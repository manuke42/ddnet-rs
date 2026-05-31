pub trait TimeDisplay {
    fn to_local_time_string(&self, short: bool) -> String;
}

#[cfg(not(target_arch = "wasm32"))]
impl TimeDisplay for chrono::DateTime<chrono::Utc> {
    fn to_local_time_string(&self, short: bool) -> String {
        if short {
            <chrono::DateTime<chrono::Local>>::from(*self)
                .format("%Y-%m-%d")
                .to_string()
        } else {
            <chrono::DateTime<chrono::Local>>::from(*self)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl TimeDisplay for chrono::DateTime<chrono::Utc> {
    fn to_local_time_string(&self, short: bool) -> String {
        if short {
            (*self).format("%Y-%m-%d").to_string()
        } else {
            self.to_string()
        }
    }
}
