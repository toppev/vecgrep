use anyhow::{Context, Result};
use clap::{ArgAction, Parser};
use model2vec_rs::model::StaticModel;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::io::IsTerminal;
use std::io::{self, BufRead};

#[derive(Parser, Debug)]
#[command(
    name = "vecgrep",
    version,
    about = "Semantic grep powered by model2vec-rs"
)]
struct Cli {
    /// Query string to search for semantically similar lines
    query: String,

    /// Similarity threshold in [0,1]. Matches below are filtered out
    #[arg(short = 't', long = "threshold", default_value_t = 0.6)]
    threshold: f32,

    /// Number of context lines to show after each match (like grep -A)
    #[arg(short = 'A', default_value_t = 0)]
    after: usize,

    /// Number of context lines to show before each match (like grep -B)
    #[arg(short = 'B', default_value_t = 0)]
    before: usize,

    /// Model ID from Hugging Face or local path (env: VECGREP_MODEL)
    #[arg(
        short = 'm',
        long = "model",
        env = "VECGREP_MODEL",
        default_value = "minishlab/potion-base-8M"
    )]
    model: String,

    /// Hide similarity score for each matching line
    #[arg(long = "hide-scores", action = ArgAction::SetTrue)]
    hide_scores: bool,

    /// Return top-N most similar lines (disables threshold; not allowed with --stream)
    #[arg(long = "top", conflicts_with = "stream")]
    top: Option<usize>,

    /// Batch size for encoding (tune perf / memory)
    #[arg(long = "batch-size", default_value_t = 1024)]
    batch_size: usize,

    /// Stream mode: process and print incrementally for non-stopping input
    #[arg(long = "stream", action = ArgAction::SetTrue)]
    stream: bool,
}

fn normalize(v: &mut [f32]) {
    let sum_sq: f32 = v.iter().map(|x| x * x).sum();
    if sum_sq > 0.0 {
        let inv = 1.0 / sum_sq.sqrt();
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    dot
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load model (normalize embeddings enabled by default config unless overridden)
    let model = StaticModel::from_pretrained(&cli.model, None, None, None)
        .context("failed to load model")?;

    // Encode query once
    let mut query_vec = model.encode(std::slice::from_ref(&cli.query))[0].clone();
    normalize(&mut query_vec);

    if cli.stream {
        run_stream(&cli, &model, &query_vec)?;
        return Ok(());
    }

    // If reading from piped stdin without --stream, print a hint once
    if !io::stdin().is_terminal() {
        eprintln!(
            "reading from stdin until EOF. For endless inputs (e.g., tail -f), use --stream to process incrementally"
        );
    }

    // Read all stdin lines first to preserve order for context windows
    let stdin = io::stdin();
    let input_lines: Vec<String> = stdin
        .lock()
        .lines()
        .collect::<Result<_, _>>()
        .context("failed reading stdin")?;

    // Encode all lines in batches; model2vec-rs exposes encode_with_args for batch tuning
    let embeddings = model.encode_with_args(&input_lines, None, cli.batch_size);

    // Normalize each embedding for cosine similarity
    let norm_embeddings: Vec<Vec<f32>> = embeddings
        .into_par_iter()
        .map(|mut v| {
            normalize(&mut v);
            v
        })
        .collect();

    // Compute similarity per line once
    let scores: Vec<f32> = norm_embeddings
        .par_iter()
        .map(|v| cosine_similarity(&query_vec, v))
        .collect();

    // Determine matches either by threshold or by top-N selection
    let mut is_match = vec![false; input_lines.len()];
    let selection_summary: String;
    if let Some(top_n) = cli.top {
        let n = top_n.min(scores.len());
        // Build index list and select top-N by score (descending)
        let mut indices: Vec<usize> = (0..scores.len()).collect();
        indices.sort_by(|&i, &j| scores[j].partial_cmp(&scores[i]).unwrap_or(Ordering::Equal));
        for &idx in indices.iter().take(n) {
            is_match[idx] = true;
        }
        let min_selected = indices
            .iter()
            .take(n)
            .map(|&i| scores[i])
            .fold(1.0f32, |acc, s| acc.min(s));
        selection_summary = format!(
            "selected top {} lines by similarity (min selected score {:.3})",
            n, min_selected
        );
    } else {
        // Identify matches above threshold
        let threshold = cli.threshold;
        let mut selection_count: usize = 0;
        for (idx, &score) in scores.iter().enumerate() {
            if score >= threshold {
                is_match[idx] = true;
                selection_count += 1;
            }
        }
        selection_summary = if selection_count == 0 {
            format!("no matches above threshold {:.2}", threshold)
        } else {
            format!("matches: {} (threshold {:.2})", selection_count, threshold)
        };
    }

    // Print matches with context, merging overlapping windows
    let mut i = 0usize;
    while i < input_lines.len() {
        if !is_match[i] {
            i += 1;
            continue;
        }

        let start = i.saturating_sub(cli.before);
        let mut end = (i + 1 + cli.after).min(input_lines.len());
        // Expand window to include subsequent nearby matches while overlapping
        let mut j = i + 1;
        while j < input_lines.len() {
            if is_match[j] {
                let candidate_start = j.saturating_sub(cli.before);
                if candidate_start <= end {
                    // overlap, extend
                    end = (j + 1 + cli.after).min(input_lines.len());
                    j += 1;
                    continue;
                }
            }
            break;
        }

        // Print block with separators similar to grep
        for k in start..end {
            let line = &input_lines[k];
            if is_match[k] {
                let score = scores[k];
                if !cli.hide_scores {
                    println!("{}\t[{:.3}]", line, score);
                } else {
                    println!("{}", line);
                }
            } else {
                println!("{}", line);
            }
        }

        // Print a separator between blocks if not at end
        if end < input_lines.len() {
            println!("--");
        }

        i = end; // continue after this block
    }

    // Summary distribution at end (overall distribution to aid threshold selection)
    let mut all_scores: Vec<f32> = scores.clone();
    all_scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let q = |p: f32| -> f32 {
        // Return the p-quantile (0..=1) using nearest-rank on sorted ascending
        if all_scores.is_empty() {
            return 0.0;
        }
        let n = all_scores.len();
        let idx = ((n as f32 - 1.0) * p).round() as usize;
        all_scores[idx]
    };

    let min_all = *all_scores.first().unwrap_or(&0.0);
    let max_all = *all_scores.last().unwrap_or(&0.0);
    let p50_all = q(0.50);
    let p90_all = q(0.90);
    let p95_all = q(0.95);
    let p99_all = q(0.99);
    let p999_all = q(0.999);

    println!("--");
    eprintln!("{}", selection_summary);
    eprintln!(
        "overall distribution (all lines): min {:.3}  p50 {:.3}  p90 {:.3}  p95 {:.3}  p99 {:.3}  p99.9 {:.3}  max {:.3}",
        min_all, p50_all, p90_all, p95_all, p99_all, p999_all, max_all
    );
    eprintln!(
        "suggested thresholds for top k%% lines: 5%%→{:.3}  1%%→{:.3}  0.1%%→{:.3}  0.01%%→{:.3}",
        p95_all,
        p99_all,
        p999_all,
        q(0.9999)
    );

    Ok(())
}

fn run_stream(cli: &Cli, model: &StaticModel, query_vec: &[f32]) -> Result<()> {
    let threshold = cli.threshold;
    let mut before_buf: VecDeque<String> = VecDeque::with_capacity(cli.before.max(1));
    let mut after_remaining: usize = 0;
    let mut printed_any: bool = false;
    let mut printed_prev_line: bool = false;

    let stdin = io::stdin();
    let lines = stdin.lock().lines();

    for line_res in lines {
        let line = line_res.context("failed reading stdin line")?;

        // Encode and score current line
        let mut emb = model.encode(std::slice::from_ref(&line))[0].clone();
        normalize(&mut emb);
        let score = cosine_similarity(query_vec, &emb);
        let is_match = score >= threshold;

        if is_match {
            // New block separator if we didn't just print a line
            if printed_any && !printed_prev_line {
                println!("--");
            }
            // Before-context only when starting a fresh block
            if !printed_prev_line && cli.before > 0 {
                for ctx in before_buf.iter() {
                    println!("{}", ctx);
                }
            }
            if !cli.hide_scores {
                println!("{}\t[{:.3}]", line, score);
            } else {
                println!("{}", line);
            }
            printed_any = true;
            printed_prev_line = true;
            after_remaining = cli.after;
        } else if after_remaining > 0 {
            println!("{}", line);
            printed_any = true;
            printed_prev_line = true;
            after_remaining -= 1;
        } else {
            printed_prev_line = false;
        }

        // Maintain before buffer
        if cli.before > 0 {
            if before_buf.len() == cli.before {
                before_buf.pop_front();
            }
            before_buf.push_back(line);
        }
    }

    Ok(())
}
