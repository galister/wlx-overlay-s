#![allow(dead_code)]

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

struct Data<DataType> {
    queue: VecDeque<DataType>,
}

#[derive(Clone)]
pub struct SyncEventQueue<DataType> {
    data: Rc<RefCell<Data<DataType>>>,
}

impl<DataType> SyncEventQueue<DataType> {
    pub fn new() -> Self {
        Self {
            data: Rc::new(RefCell::new(Data {
                queue: Default::default(),
            })),
        }
    }

    pub fn send(&self, message: DataType) {
        let mut data = self.data.borrow_mut();
        data.queue.push_back(message);
    }

    pub fn read(&self) -> Option<DataType> {
        let mut data = self.data.borrow_mut();
        data.queue.pop_front()
    }
}
