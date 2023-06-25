use anyhow::Result;
use std::sync::Arc;

pub(crate) struct VecWithCurr<A, R> {
    vec: Vec<Arc<A>>,
    curr: usize,
    last_errors: Vec<Result<R>>,
}

impl<A, R> VecWithCurr<A, R> {
    pub(crate) fn new<I>(vec: I) -> Self
    where
        I: IntoIterator<Item = A>,
    {
        VecWithCurr {
            vec: vec.into_iter().map(Arc::new).collect(),
            curr: 0,
            last_errors: vec![],
        }
    }

    pub(crate) async fn try_any_from_curr_async<F, FR>(&mut self, f: F) -> Result<R>
    where
        F: Fn(Arc<A>) -> FR,
        FR: futures::future::Future<Output = Result<R>>,
    {
        let indexes = (self.curr..self.vec.len() - 1).chain(0..self.curr);
        let mut results: Vec<Result<R>> = vec![];
        for i in indexes {
            let item = self
                .vec
                .get(i)
                .expect("try_any_from_curr_async: index out of range!");
            let item = (*item).clone();
            let result = f(item).await;
            let is_ok = result.is_ok();
            results.push(result);
            if is_ok {
                self.curr = i;
                break;
            }
        }
        let ret = results
            .pop()
            .expect("try_any_from_curr_async: for loop not run!");
        self.last_errors = results;
        ret
    }
}
