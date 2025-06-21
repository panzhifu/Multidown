use vergen::EmitBuilder;

fn main() {
    // 生成构建信息
    EmitBuilder::builder()
        .all_build()
        .all_git()
        .emit()
        .expect("Failed to generate build information");
} 