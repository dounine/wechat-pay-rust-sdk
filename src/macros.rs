#[macro_export]
macro_rules! error {
    ($name:expr)=>{
        cfg_if::cfg_if! {
            if #[cfg(feature = "debug-print")]{
                tracing::error!($name);
            }
        }
    };
   ($name:expr $(,$arg:tt)+ $(,)?) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "debug-print")]{
                tracing::error!($name, $($arg),*);
            }
        }
    };
}
#[macro_export]
macro_rules! debug {
    ($name:expr)=>{
        cfg_if::cfg_if! {
            if #[cfg(feature = "debug-print")]{
                tracing::debug!($name);
            }
        }
    };
    ($name:expr $(,$arg:tt)+ $(,)?) => {
        cfg_if::cfg_if! {
            if #[cfg(feature = "debug-print")]{
                tracing::debug!($name, $($arg),*);
            }
        }
    };
}
