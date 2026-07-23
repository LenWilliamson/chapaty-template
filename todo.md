## Template Repo

Not scope here, a todo in another repo:

```md
Act as an expert Rust backend engineer. I need to refactor a Rust CLI tool's `main` function to capture expected errors (returned via `anyhow::Result` and the `?` operator) and upload them as crash logs to Google Cloud Storage (GCS) before exiting.

### Context:

- The app runs in a containerized environment.
- When an error occurs, it should be logged to GCS at `{LOG_DIR}/execution_stderr.txt`.
- The current code uses `anyhow::Result<()>` directly on `#[tokio::main] async fn main()`.
- We want to capture the full error chain and stack trace (backtrace) provided by `anyhow`.

### Requirements:

1. **The "Outer-Inner" Main Pattern**: Refactor `main` so it no longer returns a `Result`. Instead, delegate the current application logic to a helper function (e.g., `async fn run() -> Result<()>` or inline block).
2. **Graceful Exit**: If the runner returns an `Err(err)`:
   - Print the detailed error formatted with `{:?}` to stderr (for standard container logs).
   - Format the error (including its backtrace) as a string.
   - Upload it to GCS to the path specified by the `LOG_DIR` environment variable, appending `/execution_stderr.txt`. (Provide a mock/commented async function `upload_to_gcs(path: &str, data: &[u8])` representing our GCS client).
   - Exit the process with exit code `1` using `std::process::exit(1)`.
3. **Panic Defense**: Keep or register a `std::panic::set_hook` alongside this logic to handle unexpected panics (uncaught runtime crashes) so we are 100% covered.

Please provide the refactored `main` setup using idiomatic, clean Rust.

[profile.release]
debug = 1 # Keep line tables / debug symbols

The above needs to be enabled to retain debug infos on release builds.
```

Should I use a grid serach limit? Should I pass fix 500 or smaller? Should I pass as env var in workflow (probably yes), shoudl I clamp first N or should I choose random (random is not reproducible) -> Wee need to justifiy in FAQ

```
// Call this once, as the very first thing in main(), before any Rayon
// usage anywhere - Rayon's global pool is a lazily-initialized singleton,
// so anything that touches it first (even indirectly) locks in the default
// before this override has a chance to run.
fn init_rayon() {
    if let Ok(n) = std::env::var("CHAPATY_RUNNER_CPU").and_then(|v| {
        v.parse::<usize>()
            .map_err(|_| std::env::VarError::NotPresent)
    }) {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .expect("rayon global pool already initialized - call init_rayon() first in main()");
    }
    // If CHAPATY_RUNNER_CPU isn't set (local dev, self-hosted, anywhere
    // outside the deployed Cloud Run job), do nothing and let Rayon's own
    // default auto-detection run - it's correct in every context except the
    // one this override exists for.
}


use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_pcg::Pcg64;

pub fn select_grid_subset<T>(mut agents: Vec<T>, limit: usize, run_id: &str) -> Vec<T> {
agents.shuffle(&mut rand::thread_rng());
agents.truncate(limit);

}
```


# =============================================================================
# run_job creates a per-run Cloud Run Job, execute it, and clean it up.
# 
# Sandboxing: execution environment gen1 IS the gVisor syscall sandbox. We do
# not hand-roll one. Egress is pinned to the internal VPC connector with
# vpc-egress=all-traffic and NO Cloud NAT / no default internet route, so the
# only thing the arbitrary compiled binary can reach is bq-exporter's private
# address.
# 
# If a required syscall is unsupported under gVisor, switch the annotation to
# gen2 (a microVM; still strongly isolated).
# =============================================================================
run_job:
  params: [project, region, run_id, uid, chat_id, attempt, access_token]
  steps:
    - run_env:
        assign:
          - job_id: ${"btrun-" + run_id + "-" + string(attempt)}
          - parent: ${"projects/" + project + "/locations/" + region}
          - image: ${sys.get_env("RUNNER_IMAGE_BASE") + ":" + run_id + "-" + string(attempt)}
          - log_dir: ${"gs://" + sys.get_env("INTERNAL_BUCKET") + "/users/" + uid + "/chats/" + chat_id + "/runs/" + run_id + "/attempt-" + string(attempt)}
          - out_dir: ${"gs://" + sys.get_env("RESULTS_BUCKET") + "/users/" + uid + "/chats/" + chat_id + "/results"}
          # RUNNER_* names, distinct from BUILD_TIMEOUT/BUILD_QUEUE_TTL: this is
          # the backtest task's own hard cap, unrelated to the build
          # stage's timeout. Defaults preserve current behavior.
          - run_timeout: ${sys.get_env("RUNNER_TIMEOUT", "900s")}
          - run_cpu: ${sys.get_env("RUNNER_CPU", "4")} # SMT is enabled so worst case are 2 physical cores
          - run_memory: ${sys.get_env("RUNNER_MEMORY", "8Gi")}
    - create_job:
        call: googleapis.run.v2.projects.locations.jobs.create
        args:
          parent: ${parent}
          jobId: ${job_id}
          body:
            launchStage: GA
            template:
              taskCount: 1
              template:
                maxRetries: 0 # our workflow owns retries, not Cloud Run
                timeout: ${run_timeout}
                serviceAccount: ${sys.get_env("RUNNER_SA")}
                executionEnvironment: EXECUTION_ENVIRONMENT_GEN1 # gVisor
                vpcAccess:
                  connector: ${sys.get_env("VPC_CONNECTOR")}
                  egress: ALL_TRAFFIC # no internet route on this VPC
                containers:
                  - image: ${image}
                    resources:
                      limits:
                        cpu: ${run_cpu}
                        memory: ${run_memory}
                    env:
                      - { name: CHAPATY_CREDENTIAL, value: "${access_token}" }
                      - { name: CHAPATY_METADATA_KEY, value: "${sys.get_env('CHAPATY_METADATA_KEY', 'chapaty-access-token')}" }
                      # Literal, not sys.get_env: this file is already at the
                      # 20-settings quota (see header) once CHAPATY_METADATA_KEY
                      # above is added. Changing this cap means editing this
                      # literal and redeploying the file - no less convenient
                      # than an env var, and it doesn't cost a settings slot.
                      - { name: CHAPATY_GRID_SEARCH_LIMIT, value: "400" }
                      - { name: AVAILABLE_CPUS, value: "${run_cpu}" }
                      - { name: RESULTS_DIR, value: "${out_dir}" }
                      - { name: LOG_DIR, value: "${log_dir}" }
                      - {
                          name: CHAPATY_BQEXPORTER_URL,
                          value: "${sys.get_env('CHAPATY_BQEXPORTER_URL')}",
                        }
                      # Same value already used for resources.limits.cpu above -
                      # forwarded so the runner can explicitly size its Rayon
                      # thread pool to match, instead of trusting Rayon's own
                      # CPU auto-detection inside a gVisor-sandboxed, cgroup-
                      # limited container (unverified whether it sees the true
                      # quota or the host's full core count). Not a new
                      # sys.get_env call, so it doesn't cost a settings slot -
                      # run_cpu already exists above.
                      - { name: CHAPATY_RUNNER_CPU, value: "${run_cpu}" }
                      # Without this, std::backtrace::Backtrace::force_capture()
                      # in the runner's panic hook returns an empty/disabled
                      # trace, so the execution_stderr.txt it uploads to LOG_DIR
                      # on crash would just be the panic message with no stack.
                      - { name: RUST_BACKTRACE, value: "1" }
    - run_exec:
        try:
          call: googleapis.run.v2.projects.locations.jobs.run
          args:
            name: ${parent + "/jobs/" + job_id}
          result: exec
        except:
          as: e
          steps:
            - cleanup_on_fail:
                call: delete_job_best_effort
                args:
                  name: ${parent + "/jobs/" + job_id}
            - rethrow:
                raise: ${e} # propagate so main's run_backtest `except` -> heal
    - cleanup_ok:
        call: delete_job_best_effort
        args:
          name: ${parent + "/jobs/" + job_id}
    - run_done:
        return: ${exec}
