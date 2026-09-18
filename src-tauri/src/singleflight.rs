//! Share concurrent reads only. Completed values are never retained as a cache.
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::OnceCell;

pub struct SingleFlight<K, V> {
    pending: Mutex<HashMap<K, (Weak<OnceCell<V>>, bool)>>,
}

impl<K, V> Default for SingleFlight<K, V> {
    fn default() -> Self {
        Self { pending: Mutex::new(HashMap::new()) }
    }
}

impl<K: Eq + Hash, V: Clone> SingleFlight<K, V> {
    pub async fn read<F: Future<Output = V>>(
        &self,
        key: K,
        force: bool,
        read: impl FnOnce() -> F,
    ) -> V {
        let cell = {
            let mut pending = self.pending.lock().unwrap();
            pending.retain(|_, (value, _)| value.strong_count() > 0);
            let existing = pending.get(&key)
                .filter(|(_, refreshing)| !force || *refreshing)
                .and_then(|(value, _)| Weak::upgrade(value));
            existing.unwrap_or_else(|| {
                let cell = Arc::new(OnceCell::new());
                pending.insert(key, (Arc::downgrade(&cell), force));
                cell
            })
        };
        // OnceCell releases initialization on cancellation/panic; another waiter can retry.
        cell.get_or_init(read).await.clone()
    }

    pub fn invalidate(&self) {
        self.pending.lock().unwrap().clear();
    }

    pub fn mutation(&self) -> Mutation<'_, K, V> {
        self.invalidate();
        Mutation(self)
    }
}

pub struct Mutation<'a, K: Eq + Hash, V: Clone>(&'a SingleFlight<K, V>);

impl<K: Eq + Hash, V: Clone> Drop for Mutation<'_, K, V> {
    fn drop(&mut self) { self.0.invalidate(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn concurrent_reads_share_work_but_later_reads_are_fresh() {
        let reads = SingleFlight::default();
        let calls = AtomicUsize::new(0);
        let read = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            42
        };
        let values = futures_util::future::join_all((0..12).map(|_| reads.read("root", false, read))).await;
        assert_eq!(values, vec![42; 12]);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        reads.read("root", false, read).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        futures_util::future::join_all((0..12).map(|_| reads.read("root", true, read))).await;
        assert_eq!(calls.load(Ordering::SeqCst), 3, "simultaneous forced refreshes share one new query");
    }

    #[tokio::test]
    async fn force_keys_and_mutation_do_not_join_an_old_read() {
        let reads = Arc::new(SingleFlight::default());
        let (started, wait) = tokio::sync::oneshot::channel();
        let (finish, blocked) = tokio::sync::oneshot::channel();
        let task_reads = reads.clone();
        let old = tokio::spawn(async move {
            task_reads.read("root", false, || async {
                started.send(()).unwrap();
                blocked.await.unwrap();
                1
            }).await
        });
        wait.await.unwrap();
        assert_eq!(reads.read("other", false, || async { 2 }).await, 2);
        assert_eq!(reads.read("root", true, || async { 3 }).await, 3);
        let mutation = reads.mutation();
        assert_eq!(reads.read("root", false, || async { 4 }).await, 4);
        drop(mutation);
        finish.send(()).unwrap();
        assert_eq!(old.await.unwrap(), 1);
        assert_eq!(reads.read("root", false, || async { 5 }).await, 5);
    }

    #[tokio::test]
    async fn canceled_initializer_does_not_strand_waiters() {
        let reads = Arc::new(SingleFlight::default());
        let (started, wait) = tokio::sync::oneshot::channel();
        let first_reads = reads.clone();
        let first = tokio::spawn(async move {
            first_reads.read("root", false, || async {
                started.send(()).unwrap();
                std::future::pending::<usize>().await
            }).await
        });
        wait.await.unwrap();
        let other_reads = reads.clone();
        let other = tokio::spawn(async move { other_reads.read("root", false, || async { 7 }).await });
        tokio::task::yield_now().await;
        first.abort();
        let _ = first.await;
        assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(1), other).await.unwrap().unwrap(), 7);
    }
}
