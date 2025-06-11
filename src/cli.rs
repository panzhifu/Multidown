use clap::Parser;
// multidown 命令行参数
#[derive(Parser, Debug)]
#[command(author="panzhifu", version="1.0", about="A multi-thread download tool", long_about = None)]
// 定义命令行参数

//1. Urls: Vec<String>,可以指定多个url
//2. Threads: usize,指定下载线程数
//3. Output: String,指定下载文件保存路径
//4. 指定输出文件地址，默认为当前目录
//5. 限定最快速度，默认为10M/s
//6. 指定config文件，默认为./multidown.conf
pub struct Args {
    /// 下载的文件URL (支持多个，但目前只处理第一个)
    pub urls: Vec<String>,
    #[arg(short = 'n', long, default_value_t = 4)]
    pub threads: usize,
    /// 下载文件保存路径 (默认为当前目录)
    #[arg(short='o', long, default_value_t = String::from("./"))]
    pub output: String,
    /// 限定最快下载速度，单位为MB/s (当前未实现此功能)
    #[arg(short='l', long, default_value_t = 10)]
    pub limit: usize,
    /// 指定配置文件路径 (默认为./multidown.conf，当前未实现此功能)
    #[arg(short='c', long, default_value_t = String::from("./multidown.conf"))]
    pub config: String,
}

