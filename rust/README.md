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

`deploy/` に Rust 用の unit を用意しています。環境依存（ユーザー名・パス）は `EnvironmentFile` に逃がすため、どの Linux 環境でも同じ手順で適用できます。

### 配置例

```bash
sudo mkdir -p /etc/wg-manager
sudo cp deploy/wg-manager.env.example /etc/wg-manager/wg-manager.env
sudoedit /etc/wg-manager/wg-manager.env

sudo cp deploy/wg-manager-worker.service /etc/systemd/system/
sudo cp deploy/wg-manager-web@.service /etc/systemd/system/

sudo systemctl daemon-reload
sudo systemctl enable --now wg-manager-worker
sudo systemctl enable --now wg-manager-web@<web実行ユーザー>
```

詳細は [docs/04-rust-migration.md](../docs/04-rust-migration.md) を参照してください。
