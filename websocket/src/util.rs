macro_rules! sync {
    ($future: expr) => {
        async_std::task::block_on(async { $future.await })
    };
}

pub(crate) use sync;