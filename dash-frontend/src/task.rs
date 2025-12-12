use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Clone)]
pub struct Tasks<TaskType>(Rc<RefCell<VecDeque<TaskType>>>)
where
	TaskType: Clone;

impl<TaskType: Clone + 'static> Tasks<TaskType> {
	pub fn new() -> Self {
		Self(Rc::new(RefCell::new(VecDeque::new())))
	}

	pub fn push(&self, task: TaskType) {
		self.0.borrow_mut().push_back(task);
	}

	pub fn drain(&mut self) -> VecDeque<TaskType> {
		let mut tasks = self.0.borrow_mut();
		std::mem::take(&mut *tasks)
	}
}

impl<TaskType: Clone + 'static> Default for Tasks<TaskType> {
	fn default() -> Self {
		Self::new()
	}
}
