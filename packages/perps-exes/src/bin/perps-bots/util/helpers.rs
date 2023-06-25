use anyhow::{anyhow, Result};
use std::sync::Arc;

pub(crate) struct VecWithCurr<A> {
    vec: Vec<Arc<A>>,
    curr: usize,
}

impl<A> VecWithCurr<A> {
    pub(crate) fn new<I>(vec: I) -> Self
    where
        I: IntoIterator<Item = A>,
    {
        VecWithCurr {
            vec: vec.into_iter().map(Arc::new).collect(),
            curr: 0,
        }
    }

    pub(crate) async fn try_any_from_curr_async<F, R, FR>(&mut self, f: F) -> Result<R>
    where
        F: Fn(Arc<A>) -> FR,
        FR: futures::future::Future<Output = Result<R>>,
    {
        let indexes = (self.curr..self.vec.len() - 1).chain(0..self.curr);
        let mut result: Result<R> = Err(anyhow!("nonsense"));
        for i in indexes {
            let item = self
                .vec
                .get(i)
                .expect("try_any_from_curr_async index out of range!");
            let item = (*item).clone();
            result = f(item).await;
            if result.is_ok() {
                self.curr = i;
                break;
            }
        }
        result
    }
}
