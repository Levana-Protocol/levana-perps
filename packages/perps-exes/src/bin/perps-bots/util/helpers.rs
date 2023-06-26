use anyhow::Result;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub(crate) struct VecWithCurr<A> {
    vec: Vec<Arc<A>>,
    curr: AtomicUsize,
}

impl<A> VecWithCurr<A> {
    pub(crate) fn new<I>(vec: I) -> Self
    where
        I: IntoIterator<Item = A>,
    {
        VecWithCurr {
            vec: vec.into_iter().map(Arc::new).collect(),
            curr: 0.into(),
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
                    return Ok(x);
                }
                Err(e) => results.push(e),
            }
        }

        Err(anyhow::anyhow!(
            "Unable to query any of the Pyth endpoints. All raw errors: {results:?}"
        ))
    }
}
