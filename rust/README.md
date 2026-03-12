# WireGuard Web Manager - Rust 実装

Python 版を完全にリファクタリングした Rust 実装。RHEL・Ubuntu 等で単一バイナリとして動作させる。

## ビルド

Rust ツールチェーンが必要です（[rustup](https://rustup.rs/) 推奨）。

```bash
cd rust
cargo build --release
```

- **Web**: `target/release/wg-manager`
- **Worker**: `target/release/wg-worker`

## 実行

- **Web**（プロジェクトルートの `config.yaml` を使用）:
  ```bash
  cd /path/to/wireguard-web-manager
  ./rust/target/release/wg-manager
  ```
  または `CONFIG_PATH=/path/to/config.yaml ./rust/target/release/wg-manager`

- **Worker**（root 必須）:
  ```bash
  sudo CONFIG_PATH=/path/to/config.yaml ./rust/target/release/wg-worker
  ```

## 構成

| クレート | 説明 |
|----------|------|
| `wg-common` | 設定（config.yaml）・Worker プロトコル（JSON）の共有定義 |
| `wg-manager` | Web UI（axum）・認証・Peer 管理・設定 API |
| `wg-worker` | 特権 Worker（root、UNIX ソケットで wg 操作のみ） |

Python 版の `config.yaml` および Worker の JSON プロトコルと互換です。

## systemd（Rust 用の例）

`deploy/` に Rust 用の unit 例を用意する予定。Python 版の unit の `ExecStart` を次のように変更すれば Rust で動かせます。

- **wg-manager-web.service**:  
  `ExecStart=/path/to/wireguard-web-manager/rust/target/release/wg-manager`
- **wg-manager-worker.service**:  
  `ExecStart=/path/to/wireguard-web-manager/rust/target/release/wg-worker`

詳細は [docs/04-rust-migration.md](../docs/04-rust-migration.md) を参照してください。
