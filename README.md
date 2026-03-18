# WireGuard Web Manager

ブラウザから WireGuard VPN のピアを管理できる **Rust 製 Web UI デーモン**です。
`.deb` / `.rpm` パッケージで簡単にインストールでき、systemd サービスとして動作します。

---

## アーキテクチャと設計思想

### 特権分離による安全設計

```
┌─────────────────────────────────────────────────────────┐
│  ブラウザ (HTTPS/HTTP)                                   │
└────────────────────┬────────────────────────────────────┘
                     │ HTTP
┌────────────────────▼────────────────────────────────────┐
│  wg-manager  (User: wgwm / 非特権)                      │
│  - Web UI・API サーバー（axum）                          │
│  - ピア管理 DB（SQLite）                                 │
│  - UNIX ソケット経由で Worker に指示                     │
└────────────────────┬────────────────────────────────────┘
                     │ UNIX socket (/run/wg-manager.sock)
┌────────────────────▼────────────────────────────────────┐
│  wg-worker   (User: root / 特権)                        │
│  - wg コマンドのみ実行（最小権限の原則）                  │
│  - Web UI からの sudo 不要・sudoers 設定不要             │
└─────────────────────────────────────────────────────────┘
```

Web UI プロセス（`wg-manager`）は一般ユーザー `wgwm` で動作するため、
脆弱性を突かれても root 権限には到達できません。
`wg` コマンドを直接実行する `wg-worker` だけが root として動作します。

---

## 主な機能

| 機能 | 説明 |
|------|------|
| **ピア管理** | 追加・削除・有効/無効切り替え、IP 自動採番 |
| **設定配布** | クライアント用 `.conf` ダウンロード、QR コード生成 |
| **ダッシュボード** | 接続中ピア数、WireGuard バージョン確認・更新通知 |
| **設定画面** | Web UI から `config.yaml` を編集・保存 |
| **ドキュメント** | `docs/` の Markdown マニュアルをブラウザで表示 |
| **特権分離** | wg-manager (非特権) + wg-worker (root) の 2 プロセス構成 |

---

## 動作要件

- **OS**: Linux（Ubuntu 22.04+, Debian 12+, RHEL 9+, Rocky Linux 9+, AlmaLinux 9+）
- **WireGuard**: `wireguard-tools`（`wg` コマンド）がインストール済みであること
- **systemd**: サービス管理に使用

---

## インストール（パッケージから）

### Ubuntu / Debian（.deb）

```bash
# GitHub Releases から最新の .deb をダウンロード
wget https://github.com/tm-io/WireGuard_WebManager/releases/latest/download/wireguard-webmanager_<version>_amd64.deb

# インストール（wgwm ユーザー作成・systemd 登録まで自動）
sudo dpkg -i wireguard-webmanager_<version>_amd64.deb

# WireGuard tools がない場合
sudo apt-get install -f
```

### RHEL / Rocky Linux / AlmaLinux（.rpm）

```bash
# GitHub Releases から最新の .rpm をダウンロード
wget https://github.com/tm-io/WireGuard_WebManager/releases/latest/download/wireguard-webmanager-<version>-1.x86_64.rpm

# dnf でインストール（依存関係も自動解決）
sudo dnf install wireguard-webmanager-<version>-1.x86_64.rpm

# または rpm コマンドで直接インストール
sudo rpm -ivh wireguard-webmanager-<version>-1.x86_64.rpm
```

> リリースページ: https://github.com/tm-io/WireGuard_WebManager/releases

---

## 初期設定と起動

### 1. WireGuard インターフェースを準備

インストール前に、サーバー側の WireGuard インターフェース（例: `wg0`）を手動でセットアップしてください。

```bash
# WireGuard インストール
sudo apt install wireguard-tools   # Ubuntu/Debian
sudo dnf install wireguard-tools   # RHEL 系

# サーバー鍵ペア生成
wg genkey | sudo tee /etc/wireguard/server_private.key | wg pubkey | sudo tee /etc/wireguard/server_public.key

# /etc/wireguard/wg0.conf を作成（例）
sudo tee /etc/wireguard/wg0.conf <<EOF
[Interface]
Address = 10.8.0.1/24
ListenPort = 51820
PrivateKey = $(sudo cat /etc/wireguard/server_private.key)
EOF

# インターフェース起動
sudo systemctl enable --now wg-quick@wg0
```

### 2. config.yaml を編集

インストール後、自動生成された設定ファイルを編集します。

```bash
sudo nano /etc/wireguard-webmanager/config.yaml
```

**最低限変更が必要な項目:**

```yaml
app:
  host: "0.0.0.0"           # 直接公開: 0.0.0.0 / リバースプロキシ経由: 127.0.0.1
  port: 8080
  auth_username: "admin"
  auth_password: "CHANGE_ME"   # ← 必ず変更してください

paths:
  db_path: "/var/lib/wireguard-webmanager/wg-manager.db"
  wg_worker_socket: "/run/wg-manager.sock"
  socket_owner: "wgwm"

wireguard:
  interface: "wg0"
  server_endpoint: "203.0.113.1"   # ← サーバーの公開 IP または FQDN に変更
  listen_port: 51820
  client_ip_range: "10.8.0.0/24"
  client_dns: "1.1.1.1, 8.8.8.8"
  persistent_keepalive: 25
```

### 3. サービス起動

```bash
# Worker（root）と Web UI（wgwm）を起動
sudo systemctl start wireguard-webmanager-worker
sudo systemctl start wireguard-webmanager

# 起動状態確認
sudo systemctl status wireguard-webmanager-worker
sudo systemctl status wireguard-webmanager
```

### 4. ブラウザでアクセス

```
http://<サーバーIP>:8080
```

`config.yaml` で設定した `auth_username` / `auth_password` でログインします。

---

## サービス管理

```bash
# 起動 / 停止 / 再起動
sudo systemctl start   wireguard-webmanager wireguard-webmanager-worker
sudo systemctl stop    wireguard-webmanager wireguard-webmanager-worker
sudo systemctl restart wireguard-webmanager wireguard-webmanager-worker

# 自動起動設定
sudo systemctl enable wireguard-webmanager wireguard-webmanager-worker
```

---

## トラブルシューティング

### ログ確認

```bash
# Web UI (wg-manager) のログ
journalctl -u wireguard-webmanager -f

# Worker (wg-worker) のログ
journalctl -u wireguard-webmanager-worker -f

# 起動時のエラーのみ確認
journalctl -u wireguard-webmanager --since "5 minutes ago"
```

### よくある問題

| 症状 | 確認コマンド / 対処 |
|------|---------------------|
| ポートが既に使用中 | `lsof -i :8080` で競合プロセスを確認し `app.port` を変更 |
| Worker に接続できない | `ls -la /run/wg-manager.sock` でソケットの存在と権限を確認 |
| `wg` コマンドが見つからない | `which wg` / `apt install wireguard-tools` |
| config.yaml パースエラー | `journalctl -u wireguard-webmanager` でエラー行を確認 |
| ログインできない | `config.yaml` の `auth_username` / `auth_password` を確認 |

---

## ソースからビルド

```bash
# Rust toolchain が必要（rustup 推奨）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/tm-io/WireGuard_WebManager.git
cd WireGuard_WebManager/rust

# バイナリビルド
cargo build --release --workspace

# .deb パッケージ生成
cargo install cargo-deb
cargo deb -p wg-manager --no-build

# .rpm パッケージ生成（Ubuntu 上でのクロスパッケージングも可能）
cargo install cargo-generate-rpm
cargo generate-rpm -p wg-manager
```

---

## プロジェクト構成

```
WireGuard_WebManager/
├── rust/
│   ├── wg-manager/   Web UI デーモン（axum、非特権ユーザー wgwm で動作）
│   ├── wg-worker/    特権 Worker（root で動作、wg コマンドのみ実行）
│   └── wg-common/    共有設定・プロトコル定義
├── deploy/           systemd unit ファイル
├── docs/             運用マニュアル（Web UI の「ドキュメント」からも参照可能）
└── config.example.yaml
```

詳細なドキュメントは [docs/](docs/) を参照してください。

---

## ライセンス

MIT License — 詳細は [LICENSE](LICENSE) を参照してください。
