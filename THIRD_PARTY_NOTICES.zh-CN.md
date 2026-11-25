# 第三方声明

[English](THIRD_PARTY_NOTICES.md) | **简体中文**

admin-kit 自身代码使用 MIT License（见 `LICENSE`）。它依赖/内置以下开源项目。

## Soybean Admin

- 仓库：<https://github.com/soybeanjs/soybean-admin>
- 版本：`v2.3.0`（见 `web/UPSTREAM.md`）
- License：MIT
- 原始许可证保留在 `web/LICENSE`。

## 前端依赖

| 项目 | License | 链接 |
|---|---|---|
| Vue 3 | MIT | <https://github.com/vuejs/core> |
| Vite | MIT | <https://github.com/vitejs/vite> |
| Vue Router | MIT | <https://github.com/vuejs/router> |
| Pinia | MIT | <https://github.com/vuejs/pinia> |
| Naive UI | MIT | <https://github.com/tusen-ai/naive-ui> |
| UnoCSS | MIT | <https://github.com/unocss/unocss> |
| Vue I18n | MIT | <https://github.com/intlify/vue-i18n> |

## 后端依赖

| 项目 | License | 链接 |
|---|---|---|
| axum | MIT | <https://github.com/tokio-rs/axum> |
| Tokio | MIT | <https://github.com/tokio-rs/tokio> |
| argon2 | MIT OR Apache-2.0 | <https://github.com/RustCrypto/password-hashes> |
| ed25519-dalek | BSD-3-Clause | <https://github.com/dalek-cryptography/ed25519-dalek> |
| AWS SDK for Rust | Apache-2.0 | <https://github.com/awslabs/aws-sdk-rust> |
| AWS Lambda Web Adapter | Apache-2.0 | <https://github.com/aws/aws-lambda-web-adapter> |

## 运行时/基础设施

- Amazon DynamoDB、Lambda、CloudFront、S3、SSM 属于 AWS 服务，受 AWS 客户协议约束。
  本仓库只包含配置代码。

如发现遗漏或错误，请开 issue。