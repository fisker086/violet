// 由 build.rs 从 proto 生成
// google.protobuf.Any 使用 prost-types crate，不需要生成

// prost-build 根据 package 名称生成文件
// package im.connect; 会生成 im.connect.rs
// 生成的文件内容会定义 pub mod im { pub mod connect { ... } }
// 我们需要手动创建模块结构来匹配

// 创建模块结构来匹配生成的代码
pub mod im {
    pub mod connect {
        include!(concat!(env!("OUT_DIR"), "/im.connect.rs"));
    }
}

// 重新导出 prost-types 中的 Any，方便使用
pub use prost_types::Any;
