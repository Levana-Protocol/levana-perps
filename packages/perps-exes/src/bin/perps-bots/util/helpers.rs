use anyhow::Result;
use parking_lot::RwLock;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub(crate) struct VecWithCurr<A> {
    vec: Vec<Arc<A>>,
    curr: AtomicUsize,
    last_errors: RwLock<Vec<anyhow::Error>>,
}

impl<A> VecWithCurr<A> {
    pub(crate) fn new<I>(vec: I) -> Self
    where
        I: IntoIterator<Item = A>,
    {
        VecWithCurr {
            vec: vec.into_iter().map(Arc::new).collect(),
            curr: 0.into(),
            last_errors: vec![].into(),
        }
    }

    pub(crate) async fn try_any_from_curr_async<F, FR, R>(&self, f: F) -> Result<R>
    where
        F: Fn(Arc<A>) -> FR,
        FR: futures::future::Future<Output = Result<R>>,
    {
        let curr_idx = self.curr.load(Ordering::SeqCst);

        let iter = self.vec.iter().enumerate();
        let iter = iter.clone().skip(curr_idx).chain(iter.take(curr_idx));
        let mut results = vec![];
        for (i, item) in iter {
            match f(item.clone()).await {
                Ok(x) => {
                    self.curr.store(i, Ordering::SeqCst);
                    *self.last_errors.write() = results;
                    return Ok(x);
                }
                Err(e) => results.push(e),
            }
        }

        let e = Err(anyhow::anyhow!(
            "Unable to query any of the Pyth endpoints. All raw errors: {results:?}"
        ));
        *self.last_errors.write() = results;
        e
    }
}
