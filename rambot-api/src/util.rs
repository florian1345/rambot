use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

#[derive(Clone)]
pub(crate) struct MultiJoinHandle<T: Clone> {
    join_handle: Arc<Mutex<Option<JoinHandle<T>>>>,
    result: Arc<Mutex<Option<T>>>
}

impl<T: Clone> MultiJoinHandle<T> {
    pub(crate) fn new(join_handle: JoinHandle<T>) -> MultiJoinHandle<T> {
        MultiJoinHandle {
            join_handle: Arc::new(Mutex::new(Some(join_handle))),
            result: Arc::new(Mutex::new(None))
        }
    }

    pub(crate) fn join(&self) -> T {
        let mut handle_guard = self.join_handle.lock().unwrap();

        if let Some(handle) = handle_guard.take() {
            let t = handle.join().unwrap();
            let mut result_guard = self.result.lock().unwrap();
            *result_guard = Some(t);
            result_guard.as_ref().unwrap().clone()
        }
        else {
            let result_guard = self.result.lock().unwrap();
            result_guard.as_ref().unwrap().clone()
        }
    }

    pub(crate) fn has_terminated(&self) -> bool {
        self.join_handle.lock().unwrap().is_none()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use std::thread;
    use std::time::Duration;

    #[test]
    fn multi_join() {
        let h1 = MultiJoinHandle::new(thread::spawn(|| {
            // a long computation
            thread::sleep(Duration::from_millis(50));
            42
        }));
        let h2 = h1.clone();

        assert_eq!(42, h2.join());
        assert_eq!(42, h1.join());
    }
}
