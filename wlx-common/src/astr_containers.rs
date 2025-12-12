use std::sync::Arc;

pub type AStrMap<V> = Vec<(Arc<str>, V)>;

pub trait AStrMapExt<V> {
	fn arc_set(&mut self, key: Arc<str>, value: V) -> bool;
	fn arc_get(&self, key: &str) -> Option<&V>;
	fn arc_rm(&mut self, key: &str) -> Option<V>;
}

impl<V> AStrMapExt<V> for AStrMap<V> {
	fn arc_set(&mut self, key: Arc<str>, value: V) -> bool {
		let index = self.iter().position(|(k, _)| k.as_ref().eq(key.as_ref()));
		index.map(|i| self.remove(i).1);
		self.push((key, value));
		true
	}

	fn arc_get(&self, key: &str) -> Option<&V> {
		self
			.iter()
			.find_map(|(k, v)| if k.as_ref().eq(key) { Some(v) } else { None })
	}

	fn arc_rm(&mut self, key: &str) -> Option<V> {
		let index = self.iter().position(|(k, _)| k.as_ref().eq(key));
		index.map(|i| self.remove(i).1)
	}
}

pub type AStrSet = Vec<Arc<str>>;

pub trait AStrSetExt {
	fn arc_set(&mut self, value: Arc<str>) -> bool;
	fn arc_get(&self, value: &str) -> bool;
	fn arc_rm(&mut self, value: &str) -> bool;
}

impl AStrSetExt for AStrSet {
	fn arc_set(&mut self, value: Arc<str>) -> bool {
		if self.iter().any(|v| v.as_ref().eq(value.as_ref())) {
			return false;
		}
		self.push(value);
		true
	}

	fn arc_get(&self, value: &str) -> bool {
		self.iter().any(|v| v.as_ref().eq(value))
	}

	fn arc_rm(&mut self, value: &str) -> bool {
		let index = self.iter().position(|v| v.as_ref().eq(value));
		index.is_some_and(|i| {
			self.remove(i);
			true
		})
	}
}
