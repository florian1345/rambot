use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
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

pub(crate) struct Transaction<W: Write> {
    write: Arc<Mutex<W>>,
    buffer: Vec<u8>
}

impl<W: Write> Transaction<W> {
    fn new(write: Arc<Mutex<W>>) -> Transaction<W> {
        Transaction {
            write,
            buffer: Vec::new()
        }
    }

    pub(crate) fn commit(self) -> io::Result<()> {
        self.write.lock().unwrap().write_all(&self.buffer)
    }
}

impl<W: Write> Write for Transaction<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) struct TransactionalWrite<W: Write> {
    write: Arc<Mutex<W>>
}

impl<W: Write> TransactionalWrite<W> {
    pub(crate) fn new(write: W) -> TransactionalWrite<W> {
        TransactionalWrite {
            write: Arc::new(Mutex::new(write))
        }
    }

    pub(crate) fn open_transaction(&self) -> Transaction<W> {
        Transaction::new(Arc::clone(&self.write))
    }
}

impl<W: Write> Clone for TransactionalWrite<W> {
    fn clone(&self) -> TransactionalWrite<W> {
        TransactionalWrite {
            write: Arc::clone(&self.write)
        }
    }
}

pub(crate) struct ObservableQueue<T> {
    elements: VecDeque<T>,
    senders: VecDeque<Sender<T>>
}

impl<T> ObservableQueue<T> {
    pub(crate) fn new() -> ObservableQueue<T> {
        ObservableQueue {
            elements: VecDeque::new(),
            senders: VecDeque::new()
        }
    }

    pub(crate) fn enqueue(&mut self, mut element: T) {
        while let Some(sender) = self.senders.pop_front() {
            match sender.send(element) {
                Ok(_) => return,
                Err(e) => element = e.0
            }
        }

        self.elements.push_back(element);
    }

    pub(crate) fn dequeue(&mut self) -> Option<T> {
        self.elements.pop_front()
    }

    pub(crate) fn observe(&mut self) -> Receiver<T> {
        let (sender, receiver) = mpsc::channel();

        if let Some(element) = self.dequeue() {
            sender.send(element).unwrap();
        }
        else {
            self.senders.push_back(sender);
        }

        receiver
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use std::iter;
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

    struct VecWrite {
        vec: Arc<Mutex<Vec<u8>>>
    }

    impl Write for VecWrite {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.vec.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn interleaved_transactions() {
        const THREAD_COUNT: usize = 64;
        let vec = Arc::new(Mutex::new(Vec::new()));
        let write = VecWrite {
            vec: Arc::clone(&vec)
        };
        let write = TransactionalWrite::new(write);
        let mut join_handles = Vec::new();

        for _ in 0..THREAD_COUNT {
            let write = write.clone();

            join_handles.push(thread::spawn(move || {
                let mut tx = write.open_transaction();
                tx.write_all(&[0]).unwrap();
                thread::sleep(Duration::from_millis(10));
                tx.write_all(&[1]).unwrap();
                tx.commit().unwrap();
            }));
        }

        for join_handle in join_handles {
            join_handle.join().unwrap();
        }

        let actual = vec.lock().unwrap().iter()
            .cloned()
            .collect::<Vec<_>>();
        let expected = iter::repeat(vec![0u8, 1u8])
            .take(THREAD_COUNT)
            .flat_map(|v| v.into_iter())
            .collect::<Vec<_>>();
        assert_eq!(expected, actual);
    }

    #[test]
    fn observable_queue_filled_on_observe() {
        let mut queue = ObservableQueue::new();
        queue.enqueue(42);
        queue.enqueue(43);
        let r = queue.observe();
        assert_eq!(42, r.recv().unwrap());
        assert_eq!(Some(43), queue.dequeue());
    }

    #[test]
    fn observable_queue_empty_on_observe() {
        let mut queue = ObservableQueue::new();
        let r = queue.observe();

        thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            queue.enqueue(42);
        });

        assert_eq!(42, r.recv().unwrap());
    }
}
