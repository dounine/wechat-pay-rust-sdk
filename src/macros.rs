#[cfg(feature = "debug-print")]
#[macro_export]
macro_rules! error {
    ($name:expr)=>{
        tracing::error!($name);
    };
   ($name:expr $(,$arg:tt)+) => {
       tracing::error!($name, $($arg),*);
    };
}
#[cfg(not(feature = "debug-print"))]
#[macro_export]
macro_rules! error {
    ($name:expr) => {};
    ($name:expr $(,$arg:tt)+ $(,)?) => {};
}

#[cfg(feature = "debug-print")]
#[macro_export]
macro_rules! debug {
    ($name:expr)=>{
        tracing::debug!($name);
    };
    ($name:expr $(,$arg:tt)+) => {
        tracing::debug!($name, $($arg),*);
    };
}

#[cfg(not(feature = "debug-print"))]
#[macro_export]
macro_rules! debug {
    ($name:expr) => {};
    ($name:expr $(,$arg:tt)+ $(,)?) => {};
}
