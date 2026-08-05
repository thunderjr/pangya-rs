//! Cleanup-path, DB-readiness, and retry-schedule acceptance tests.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use pangya_observability::{HealthState, M2Metrics};
use pangya_server::{
    ServerError, prepare_public_bind, retry_schedule, run_database_probe, run_readiness_probe,
    supervise_tasks,
};
use pangya_storage::{MIGRATOR, PgRepository};
use sqlx::PgPool;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

fn ready_health() -> Arc<HealthState> {
    let health = Arc::new(HealthState::new(
        Arc::new(M2Metrics::default()),
        Duration::from_secs(10),
        true,
    ));
    health.set_config_valid(true);
    health.set_database_migrated(true);
    health.set_login_bound(true);
    health
}

#[tokio::test]
async fn signal_success_error_and_required_task_failure_share_readiness_first_cleanup() {
    let health = ready_health();
    let shutdown = CancellationToken::new();
    let mut tasks = JoinSet::new();
    let child = shutdown.child_token();
    tasks.spawn(async move {
        child.cancelled().await;
        Ok(())
    });
    let status = supervise_tasks(
        tasks,
        async { Ok(()) },
        Arc::clone(&health),
        shutdown.clone(),
        Duration::from_secs(1),
    )
    .await;
    assert!(status.is_ok());
    assert!(!health.ready());
    assert!(shutdown.is_cancelled());

    let health = ready_health();
    let shutdown = CancellationToken::new();
    let mut tasks = JoinSet::new();
    let child = shutdown.child_token();
    tasks.spawn(async move {
        child.cancelled().await;
        Ok(())
    });
    let status = supervise_tasks(
        tasks,
        async { Err(ServerError::Signal) },
        Arc::clone(&health),
        shutdown.clone(),
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(status, Err(ServerError::Signal)));
    assert!(!health.ready());
    assert!(shutdown.is_cancelled());

    let health = ready_health();
    let shutdown = CancellationToken::new();
    let mut tasks = JoinSet::new();
    tasks.spawn(async { Err(ServerError::Database) });
    let status = supervise_tasks(
        tasks,
        std::future::pending(),
        Arc::clone(&health),
        shutdown.clone(),
        Duration::from_secs(1),
    )
    .await;
    assert!(matches!(status, Err(ServerError::Database)));
    assert!(!health.ready());
    assert!(shutdown.is_cancelled());
}

#[tokio::test]
async fn successful_signal_does_not_hide_cancelled_required_task_failure() {
    let health = ready_health();
    let shutdown = CancellationToken::new();
    let child = shutdown.child_token();
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        child.cancelled().await;
        Err(ServerError::Database)
    });
    let status = supervise_tasks(
        tasks,
        async { Ok(()) },
        Arc::clone(&health),
        shutdown,
        Duration::from_millis(20),
    )
    .await;
    assert!(matches!(status, Err(ServerError::Database)));
    assert!(!health.ready());
}

#[tokio::test]
async fn supervisor_propagates_login_service_style_over_grace_cleanup_failure() {
    let health = ready_health();
    let shutdown = CancellationToken::new();
    let completed = Arc::new(AtomicBool::new(false));
    let task_completed = Arc::clone(&completed);
    let child = shutdown.child_token();
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        child.cancelled().await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        task_completed.store(true, Ordering::Release);
        Err(ServerError::Runtime)
    });
    let status = supervise_tasks(
        tasks,
        async { Ok(()) },
        Arc::clone(&health),
        shutdown,
        Duration::from_millis(50),
    )
    .await;
    assert!(matches!(status, Err(ServerError::Runtime)));
    assert!(completed.load(Ordering::Acquire));
    assert!(!health.ready());
}

#[tokio::test]
async fn injected_readiness_probe_reflects_failure_then_recovery() {
    let health = ready_health();
    let available = Arc::new(AtomicBool::new(false));
    let shutdown = CancellationToken::new();
    let task_health = Arc::clone(&health);
    let task_available = Arc::clone(&available);
    let task_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        run_readiness_probe(
            task_health,
            task_shutdown,
            Duration::from_millis(5),
            Duration::from_millis(50),
            move || {
                let ready = task_available.load(Ordering::Acquire);
                async move { ready }
            },
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while health.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("probe failure");
    available.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !health.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("probe recovery");
    shutdown.cancel();
    task.await.expect("join").expect("probe");
}

#[tokio::test]
async fn readiness_probe_times_out_pending_work_then_recovers() {
    let health = ready_health();
    let pending = Arc::new(AtomicBool::new(true));
    let shutdown = CancellationToken::new();
    let task_pending = Arc::clone(&pending);
    let task = tokio::spawn(run_readiness_probe(
        Arc::clone(&health),
        shutdown.clone(),
        Duration::from_millis(5),
        Duration::from_millis(10),
        move || {
            let should_pending = task_pending.load(Ordering::Acquire);
            async move {
                if should_pending {
                    std::future::pending::<()>().await;
                }
                true
            }
        },
    ));
    tokio::time::timeout(Duration::from_secs(1), async {
        while health.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("pending probe timeout");
    pending.store(false, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !health.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("probe recovered");
    shutdown.cancel();
    task.await.expect("join").expect("probe");
}

#[test]
fn exponential_retry_schedule_has_exact_attempt_count_and_cap() {
    assert_eq!(
        retry_schedule(5, Duration::from_millis(100), Duration::from_millis(250))
            .expect("schedule"),
        [
            Duration::from_millis(100),
            Duration::from_millis(200),
            Duration::from_millis(250),
            Duration::from_millis(250),
        ]
    );
    assert!(
        retry_schedule(1, Duration::from_secs(1), Duration::from_secs(2))
            .expect("single")
            .is_empty()
    );
    assert!(matches!(
        retry_schedule(u32::MAX, Duration::from_secs(1), Duration::from_secs(2)),
        Err(ServerError::InvalidRetry)
    ));
    assert!(matches!(
        retry_schedule(2, Duration::from_secs(3), Duration::from_secs(2)),
        Err(ServerError::InvalidRetry)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn public_bind_audit_failure_prevents_preparation(pool: PgPool) {
    sqlx::query(
        "CREATE FUNCTION test_fail_public_audit() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected public audit failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("failure function");
    sqlx::query(
        "CREATE TRIGGER test_fail_public_audit BEFORE INSERT ON operator_audit_events \
         FOR EACH ROW EXECUTE FUNCTION test_fail_public_audit()",
    )
    .execute(&pool)
    .await
    .expect("failure trigger");
    let repository = PgRepository::new(pool);
    assert!(matches!(
        prepare_public_bind(&repository, true).await,
        Err(ServerError::Audit)
    ));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn continuous_database_probe_turns_readiness_false_after_pool_close(pool: PgPool) {
    let health = ready_health();
    let shutdown = CancellationToken::new();
    let task = tokio::spawn(run_database_probe(
        pool.clone(),
        Arc::clone(&health),
        shutdown.clone(),
        Duration::from_millis(5),
        Duration::from_millis(50),
    ));
    tokio::time::timeout(Duration::from_secs(1), async {
        while !health.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("initial readiness");
    pool.close().await;
    tokio::time::timeout(Duration::from_secs(1), async {
        while health.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("readiness false");
    shutdown.cancel();
    task.await.expect("probe join").expect("probe result");
}
