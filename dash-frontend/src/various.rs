pub fn get_username() -> String {
	match std::env::var("USER") {
		Ok(user) => user,
		Err(_) => String::from("anonymous"),
	}
}
