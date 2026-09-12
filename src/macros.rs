/// adds on an expr to the existing .droptube directory
#[macro_export]
macro_rules! droptube_dir {
    ($path:expr) => {
        concat!(".droptube", $path)
    };
}
