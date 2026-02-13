fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    
    // 打印 OUT_DIR 路径（构建时可见）
    println!("cargo:warning=OUT_DIR = {}", out_dir);
    
    // 使用 protoc-bin-vendored 提供的 protoc（适用于 musl/Docker 构建）
    // 设置 PROTOC 环境变量，prost-build 会自动使用
    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", &protoc_path);
    
    // 配置 prost-build：使用 prost-types 处理 google.protobuf 类型
    let mut config = prost_build::Config::new();
    
    // 只编译我们的 proto 文件，google.protobuf.Any 会自动使用 prost-types
    config
        .out_dir(&out_dir)
        .compile_protos(
            &["proto/im_message_wrap.proto"],
            &["proto"],
        )?;
    
    // 打印生成的文件（用于调试）
    println!("cargo:warning=Generated files in OUT_DIR");
    Ok(())
}
