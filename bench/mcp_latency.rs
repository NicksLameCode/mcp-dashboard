//! MCP Latency Benchmark
//!
//! Compares persistent-connection (dashboard) vs cold-start (traditional) MCP interaction.
//!
//! Usage: cargo run --release --bin mcp-latency -- [server_command] [args...]
//! Example: cargo run --release --bin mcp-latency -- /path/to/db-tunnel mcp

use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use std::time::{Duration, Instant};
use tokio::process::Command;

const WARMUP_ROUNDS: usize = 2;
const COLD_START_ROUNDS: usize = 20;
const WARM_CALL_ROUNDS: usize = 50;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

struct Stats {
    min: f64,
    max: f64,
    mean: f64,
    p50: f64,
    p95: f64,
    p99: f64,
}

fn compute_stats(mut samples: Vec<f64>) -> Stats {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = samples.len();
    let sum: f64 = samples.iter().sum();
    Stats {
        min: samples[0],
        max: samples[n - 1],
        mean: sum / n as f64,
        p50: samples[n / 2],
        p95: samples[(n as f64 * 0.95) as usize],
        p99: samples[((n as f64 * 0.99) as usize).min(n - 1)],
    }
}

async fn cold_start_once(command: &str, args: &[String]) -> Result<(f64, f64, f64, usize), String> {
    let total_start = Instant::now();

    // Spawn + initialize
    let spawn_start = Instant::now();
    let cmd = Command::new(command);
    let transport = TokioChildProcess::builder(cmd.configure(|cmd| {
        cmd.args(args);
    }))
    .spawn()
    .map_err(|e| format!("spawn failed: {e}"))?;

    let mut service = tokio::time::timeout(CONNECT_TIMEOUT, ().serve(transport.0))
        .await
        .map_err(|_| "init timeout".to_string())?
        .map_err(|e| format!("init failed: {e}"))?;

    let peer = service.peer().clone();
    let init_ms = spawn_start.elapsed().as_secs_f64() * 1000.0;

    // List tools
    let list_start = Instant::now();
    let tools = tokio::time::timeout(CALL_TIMEOUT, peer.list_all_tools())
        .await
        .map_err(|_| "list timeout".to_string())?
        .map_err(|e| format!("list failed: {e}"))?;
    let list_ms = list_start.elapsed().as_secs_f64() * 1000.0;

    let tool_count = tools.len();

    // Shutdown
    let _ = service.close().await;

    let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    Ok((total_ms, init_ms, list_ms, tool_count))
}

async fn warm_call_once(peer: &rmcp::Peer<RoleClient>) -> Result<f64, String> {
    let start = Instant::now();
    tokio::time::timeout(CALL_TIMEOUT, peer.list_all_tools())
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|e| format!("failed: {e}"))?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <server_command> [args...]", args[0]);
        eprintln!("Example: {} /path/to/db-tunnel mcp", args[0]);
        std::process::exit(1);
    }

    let command = &args[1];
    let server_args: Vec<String> = args[2..].to_vec();
    let server_name = std::path::Path::new(command)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| command.clone());

    println!("============================================================");
    println!("  MCP Latency Benchmark");
    println!("  Server: {server_name}");
    println!("  Command: {command} {}", server_args.join(" "));
    println!("============================================================");
    println!();

    // ── Phase 1: Cold Start (Traditional MCP Pattern) ───────────────────
    println!("Phase 1: Cold Start (spawn + init + list_tools + shutdown)");
    println!("  This simulates the TRADITIONAL approach where each interaction");
    println!("  spawns a new server process, initializes MCP, makes a call,");
    println!("  then kills the process.");
    println!();

    // Warmup
    print!("  Warming up ({WARMUP_ROUNDS} rounds)...");
    for _ in 0..WARMUP_ROUNDS {
        let _ = cold_start_once(command, &server_args).await;
    }
    println!(" done");

    // Benchmark
    let mut cold_total = Vec::new();
    let mut cold_init = Vec::new();
    let mut cold_list = Vec::new();
    let mut tool_count = 0;

    print!("  Running {COLD_START_ROUNDS} rounds");
    for i in 0..COLD_START_ROUNDS {
        match cold_start_once(command, &server_args).await {
            Ok((total, init, list, tc)) => {
                cold_total.push(total);
                cold_init.push(init);
                cold_list.push(list);
                tool_count = tc;
                print!(".");
            }
            Err(e) => {
                eprintln!("\n  Round {i} failed: {e}");
            }
        }
        // Small gap to avoid overwhelming the system
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    println!(" done");

    if cold_total.is_empty() {
        eprintln!("No cold start rounds succeeded. Is the server command correct?");
        std::process::exit(1);
    }

    let cold_total_stats = compute_stats(cold_total.clone());
    let cold_init_stats = compute_stats(cold_init);
    let cold_list_stats = compute_stats(cold_list);

    println!();
    println!("  Results ({} rounds, {} tools):", cold_total.len(), tool_count);
    println!("  ┌────────────────────┬──────────┬──────────┬──────────┬──────────┐");
    println!("  │ Phase              │    Mean  │    p50   │    p95   │    p99   │");
    println!("  ├────────────────────┼──────────┼──────────┼──────────┼──────────┤");
    println!(
        "  │ Spawn + Init       │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │",
        cold_init_stats.mean, cold_init_stats.p50, cold_init_stats.p95, cold_init_stats.p99
    );
    println!(
        "  │ list_tools          │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │",
        cold_list_stats.mean, cold_list_stats.p50, cold_list_stats.p95, cold_list_stats.p99
    );
    println!(
        "  │ TOTAL (cold start)  │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │",
        cold_total_stats.mean, cold_total_stats.p50, cold_total_stats.p95, cold_total_stats.p99
    );
    println!("  └────────────────────┴──────────┴──────────┴──────────┴──────────┘");

    // ── Phase 2: Warm Call (Dashboard Persistent Connection) ────────────
    println!();
    println!("Phase 2: Warm Call (persistent connection, reuse for every call)");
    println!("  This simulates the DASHBOARD approach: connect once, then make");
    println!("  repeated calls over the same persistent connection.");
    println!();

    // Establish persistent connection
    print!("  Connecting...");
    let cmd = Command::new(command);
    let transport = TokioChildProcess::builder(cmd.configure(|cmd| {
        cmd.args(&server_args);
    }))
    .spawn()?;

    let mut service = tokio::time::timeout(CONNECT_TIMEOUT, ().serve(transport.0))
        .await??;
    let peer = service.peer().clone();
    println!(" connected");

    // Warmup
    print!("  Warming up ({WARMUP_ROUNDS} rounds)...");
    for _ in 0..WARMUP_ROUNDS {
        let _ = warm_call_once(&peer).await;
    }
    println!(" done");

    // Benchmark
    let mut warm_times = Vec::new();
    print!("  Running {WARM_CALL_ROUNDS} rounds");
    for i in 0..WARM_CALL_ROUNDS {
        match warm_call_once(&peer).await {
            Ok(ms) => {
                warm_times.push(ms);
                print!(".");
            }
            Err(e) => {
                eprintln!("\n  Round {i} failed: {e}");
            }
        }
    }
    println!(" done");

    let _ = service.close().await;

    if warm_times.is_empty() {
        eprintln!("No warm rounds succeeded.");
        std::process::exit(1);
    }

    let warm_stats = compute_stats(warm_times.clone());

    println!();
    println!("  Results ({} rounds):", warm_times.len());
    println!("  ┌────────────────────┬──────────┬──────────┬──────────┬──────────┐");
    println!("  │ Phase              │    Mean  │    p50   │    p95   │    p99   │");
    println!("  ├────────────────────┼──────────┼──────────┼──────────┼──────────┤");
    println!(
        "  │ list_tools (warm)   │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │ {:>6.1}ms │",
        warm_stats.mean, warm_stats.p50, warm_stats.p95, warm_stats.p99
    );
    println!("  └────────────────────┴──────────┴──────────┴──────────┴──────────┘");

    // ── Summary ─────────────────────────────────────────────────────────
    println!();
    println!("============================================================");
    println!("  SUMMARY: {server_name}");
    println!("============================================================");
    println!();

    let speedup = cold_total_stats.mean / warm_stats.mean;
    let saved = cold_total_stats.mean - warm_stats.mean;
    let init_pct = (cold_init_stats.mean / cold_total_stats.mean) * 100.0;

    println!("  Cold start (traditional):  {:>7.1}ms  mean", cold_total_stats.mean);
    println!("    ├─ Spawn + Init:         {:>7.1}ms  ({:.0}% of total)", cold_init_stats.mean, init_pct);
    println!("    └─ list_tools:           {:>7.1}ms", cold_list_stats.mean);
    println!();
    println!("  Warm call (dashboard):     {:>7.1}ms  mean", warm_stats.mean);
    println!();
    println!("  Speedup:                   {:>7.1}x  faster", speedup);
    println!("  Saved per call:            {:>7.1}ms", saved);
    println!("  Over 100 calls saved:      {:>7.0}ms  ({:.1}s)", saved * 100.0, saved * 100.0 / 1000.0);
    println!();

    // ── Phase 3: Throughput test ────────────────────────────────────────
    println!("Phase 3: Throughput Burst (10 rapid calls on persistent connection)");
    println!();

    let cmd = Command::new(command);
    let transport = TokioChildProcess::builder(cmd.configure(|cmd| {
        cmd.args(&server_args);
    }))
    .spawn()?;
    let mut service = tokio::time::timeout(CONNECT_TIMEOUT, ().serve(transport.0))
        .await??;
    let peer = service.peer().clone();

    // Warmup
    let _ = warm_call_once(&peer).await;

    let burst_start = Instant::now();
    let mut burst_count = 0u32;
    for _ in 0..10 {
        if warm_call_once(&peer).await.is_ok() {
            burst_count += 1;
        }
    }
    let burst_ms = burst_start.elapsed().as_secs_f64() * 1000.0;
    let _ = service.close().await;

    println!("  {burst_count} calls in {burst_ms:.1}ms ({:.1}ms/call, {:.0} calls/sec)",
        burst_ms / burst_count as f64,
        burst_count as f64 / (burst_ms / 1000.0));
    println!();

    // ── Comparison for 10 calls cold vs persistent ──────────────────────
    let cold_10 = cold_total_stats.mean * 10.0;
    println!("  Comparison for 10 tool calls:");
    println!("    Traditional (10 cold starts): {:>8.0}ms ({:.1}s)", cold_10, cold_10 / 1000.0);
    println!("    Dashboard (1 connect + 10):   {:>8.0}ms ({:.1}s)",
        cold_init_stats.mean + burst_ms,
        (cold_init_stats.mean + burst_ms) / 1000.0);
    println!("    Speedup:                      {:>8.1}x", cold_10 / (cold_init_stats.mean + burst_ms));
    println!();

    Ok(())
}
