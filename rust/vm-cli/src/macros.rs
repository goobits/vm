#[macro_export]
macro_rules! msg {
    ($template:expr) => {
        $crate::builder::MessageBuilder::new($template).build()
    };
    ($template:expr, $($key:ident = $value:expr),+ $(,)?) => {
        {
            let mut builder = $crate::builder::MessageBuilder::new($template);
            $(
                builder = builder.var(stringify!($key), $value);
            )+
            builder.build()
        }
    };
}
