use std::{backtrace::Backtrace, panic, sync::LazyLock};

use anyhow::{Result, bail};
use object_store::{ObjectStoreExt, gcp::GoogleCloudStorageBuilder, path::Path as ObjectPath};

/// Directory for crash logs.
static GCP_CRASH_LOG_DIR: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("GCP_CRASH_LOG_DIR").ok());

/// Prints the full error chain + backtrace to stderr for container logs,
/// ships the same text to GCS when `GCP_CRASH_LOG_DIR` is configured, then exits.
pub async fn handle_fatal_error(err: anyhow::Error) -> ! {
    let body = format!("{err:?}");
    eprintln!("{body}");
    upload_crash_log(body).await;
    std::process::exit(1);
}

/// Registers a panic hook that reports uncaught panics the same way
/// `handle_fatal_error` reports `anyhow::Error`s, so a crash log always
/// lands in GCS regardless of whether the failure was an `Err` or a panic.
pub fn install_panic_hook() {
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |info| {
        // Keep the default formatting/behavior (stderr message, location, etc.).
        default_hook(info);
        if GCP_CRASH_LOG_DIR.is_some() {
            let body = format!("{info}\n\nStack Backtrace:\n{}", Backtrace::capture());

            let upload_thread = std::thread::Builder::new()
                .name("panic-crash-log-upload".to_string())
                .spawn(move || {
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(rt) => rt.block_on(upload_crash_log(body)),
                        Err(e) => {
                            eprintln!("Failed to start runtime for panic log upload: {e:?}")
                        }
                    }
                });

            match upload_thread {
                Ok(handle) => match handle.join() {
                    Ok(_) => {}
                    Err(e) => eprintln!("Failed to join thread for panic log upload: {e:?}"),
                },
                Err(e) => eprintln!("Failed to spawn thread for panic log upload: {e:?}"),
            }
        }

        std::process::exit(1);
    }));
}

/// Uploads `body` to `{GCP_CRASH_LOG_DIR}/execution_stderr.txt`. A no-op when
/// `GCP_CRASH_LOG_DIR` isn't set.
async fn upload_crash_log(body: String) {
    let Some(dir) = GCP_CRASH_LOG_DIR.as_deref() else {
        return;
    };

    let path = format!("{}/execution_stderr.txt", dir.trim_end_matches('/'));
    if let Err(upload_err) = upload_to_gcs(&path, body).await {
        eprintln!("Failed to upload crash log to {path}: {upload_err:?}");
    }
}

async fn upload_to_gcs(path: &str, data: String) -> Result<()> {
    let Ok(bucket_name) = std::env::var("INTERNAL_BUCKET") else {
        bail!("INTERNAL_BUCKET env var not set")
    };
    let store = GoogleCloudStorageBuilder::from_env()
        .with_bucket_name(&bucket_name)
        .build()?;

    let object_path = ObjectPath::from(path);
    store.put(&object_path, data.into()).await?;
    Ok(())
}
