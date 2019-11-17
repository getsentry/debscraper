macro_rules! log_stage {
    ($num:expr, $($arg:expr),*) => {
        println!(
            "[{}] {}",
            console::style($num).cyan().bold(),
            format_args!($($arg,)*),
        );
    }
}

macro_rules! log_result {
    ($($arg:expr),*) => {
        println!("--> {}", format_args!($($arg,)*));
    }
}