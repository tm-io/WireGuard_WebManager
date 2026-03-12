# WireGuard Web管理アプリケーション

## 現在のメイン実装（Rust）

メイン実装は **Rust 版**です。

- Rust 実装: `rust/`（ビルド・起動: [rust/README.md](rust/README.md)）
- Python 版: `archive/python/`（アーカイブ）

## 運用・マニュアル

**導入・設定変更・日常運用**は次のマニュアルを参照してください。

- **[docs/README.md](docs/README.md)** … 運用マニュアルの目次
  - 初回セットアップ: [docs/01-setup.md](docs/01-setup.md)
  - config.yaml のカスタマイズ: [docs/02-config.md](docs/02-config.md)
  - 運用イメージ（起動・停止・Worker）: [docs/03-operation.md](docs/03-operation.md)
  - Worker による権限分離: [docs/WORKER.md](docs/WORKER.md)
  - **Rust 移行方針**（完全リファクタ・マルチ環境・最小依存）: [docs/04-rust-migration.md](docs/04-rust-migration.md)

ログイン後は **ダッシュボード**・**ピア一覧**・**設定**・**ドキュメント**（本マニュアルの Web 表示）をサイドバーから利用できます。マニュアルは画面の「ドキュメント」からも参照可能です（`/manual`）。

---

## 現在の実装状況

### 実装済み

| 区分 | 内容 |
|------|------|
| **認証** | セッション認証。クレデンシャルは config.yaml から読み込み。 |
| **Peer 管理** | 追加・削除・有効/無効切り替え。IP 自動採番。鍵生成。ダウンタイムなしの動的反映（`wg set` / Worker 経由）。 |
| **設定出力** | クライアント用 .conf の**ダウンロード**（Windows/Mac/Linux）。**QR コード**（スマホ）。 |
| **権限分離** | **Worker**（root）が UNIX ソケットで wg 操作を担当。sudoers 不要。 |
| **運用** | Web アプリ・Worker を **systemd** で起動可能（deploy/ の unit ファイル）。 |
| **設定画面** | ログイン後「設定」から config.yaml の項目を編集・保存。 |
| **マニュアル表示** | 「ドキュメント」から docs フォルダの Markdown をブラウザで参照（`/manual`）。 |
| **WireGuard バージョン** | ダッシュボードで現在のバージョン表示。最新版と比較し古い場合は警告表示。 |
| **ログ** | Peer 有効/無効の失敗時などに syslog / journald へエラー出力。 |

### 未実装（WireGuard 関連）

| 機能 | 説明 |
|------|------|
| **wg0.conf の生成・保存** | サーバー側の WireGuard 設定ファイルを Web から生成・書き出し。現在はサーバーを手動で構築する想定。 |
| **インターフェースの起動・停止** | `systemctl start/stop wg-quick@wg0` などを Web/API から実行。起動・停止はサーバー上で手動。 |
| **Peer ごとの AllowedIPs 変更** | クライアントごとに AllowedIPs を変える機能。現在は全クライアント共通。 |
| **PyInstaller 単一バイナリ** | 配布用のビルドスクリプト（build.sh / .spec）は未整備。 |

---

## 開発要件・手順書（参考）

### 0. Rust 移行の命令（方針）
本プロジェクトは **Python を Rust で完全にリファクタリング** する。  
- **マルチ環境**: RHEL・Ubuntu 等でそのまま動作させる。  
- **ライブラリに依存せず**: 実行時に Python や多数のシステムライブラリに依存せず、Rust でビルドした単一（または少数）バイナリで動作させる。ビルド時のクレートも必要最小限とする。  

詳細は **[docs/04-rust-migration.md](docs/04-rust-migration.md)** に記載する。

### 1. プロジェクトの目的
Linux（Ubuntu）サーバー上で稼働するWireGuardの構築、およびクライアント（Peer）の証明書・鍵管理をブラウザから直感的に行えるWebアプリケーションをフルスクラッチで開発する。将来的な単一バイナリ配布（PyInstaller等／Rust 移行後はスタティックバイナリ）を前提としたポータブルな設計とする。

## 2. 既存環境との共存と制約事項（重要）
本サーバーには既に **Zabbix** および **Gradle** が稼働している。既存環境を破壊しないため以下の制約を厳守すること。

* **設定の外部化:** アプリケーションの待受ポート、DBパス等はソースコードにハードコードせず、`config.yaml` から読み込むこと。既存Webサーバー（ポート80/443等）との競合を回避する。
* **環境非干渉:** 既存のApache/Nginx設定、ZabbixのDB（MySQL等）には一切触れない。本アプリのDBは **SQLite3** のみを使用する。
* **配布を見据えたパス解決:** PyInstallerで単一実行ファイル化することを前提とし、静的ファイルやテンプレートの読み込みには `sys._MEIPASS` を考慮したパス解決ユーティリティ（`pathlib`利用）を必ず実装すること。

## 3. 設定ファイル (config.yaml) の必須要件
アプリケーション起動時に読み込む `config.yaml` には、最低限以下の項目を定義する。詳細は [docs/02-config.md](docs/02-config.md)。画面の「設定」からも編集・保存可能。

```yaml
app:
  host: "0.0.0.0"         # 直接受ける場合は 0.0.0.0、リバースプロキシ経由なら 127.0.0.1
  port: 8080
  auth_username: "admin"
  auth_password: "password123"

paths:
  db_path: "data/wg-manager.db"
  wg_conf_dir: "/etc/wireguard"
  wg_worker_socket: "/var/run/wg-manager.sock"  # Worker 利用（推奨）。空で sudo 利用
  socket_owner: "kanri"   # Web アプリを動かすユーザー名

wireguard:
  interface: "wg0"
  server_endpoint: "203.0.113.1"
  listen_port: 51820
  client_ip_range: "10.8.0.0/24"
  client_dns: "1.1.1.1, 8.8.8.8"
  persistent_keepalive: 25
```

## 4. コア機能（要件と現状）

- **認証** … 要望どおりセッション認証を実装。クレデンシャルは config.yaml。
- **WireGuard サーバー管理** … `wg show` による公開鍵・ステータス取得は実装済み。wg0.conf の生成・保存、インターフェースの起動/停止は未実装（上記「未実装」参照）。
- **Peer 管理** … IP 自動採番、追加・削除・無効化、鍵生成、`wg set` による動的反映を実装。Worker 経由で sudo 不要。
- **設定出力** … .conf ダウンロード（PC 用）、QR コード（スマホ用）を実装。

## 5. AI（Cursor）への開発指示ステップ

以下のフェーズに沿って、段階的に実装と検証を進めること。

### Phase 1: ベース設計と認証機能
1.  仮想環境を作成し、`FastAPI`, `uvicorn`, `pyyaml`, `jinja2`, `qrcode` などをインストール。
2.  `pathlib` と `sys._MEIPASS` を考慮したパス解決モジュールを作成。
3.  `config.yaml` を読み込む設定管理クラスを実装。
4.  ログイン画面を実装し、認証に成功したユーザーのみがダッシュボードにアクセスできるAPIルーターを作成する。

### Phase 2: OSコマンドとネットワーク連携
1.  `wg genkey`, `wg pubkey`, `wg genpsk` を実行するユーティリティを作成。
2.  IPアドレスの自動採番ロジック（`ipaddress` モジュールを利用し、DB上の既存IPと照合して空きを探す）を実装。
3.  設定変更時に `wg syncconf` (または `wg set`) を呼び出し、動的に反映させる関数を実装。

### Phase 3: データベースとAPI
1.  SQLiteに `peers` テーブル（id, name, public_key, private_key_encrypted, pre_shared_key, allocated_ip, is_active, created_at）を作成。
2.  クライアントのCRUD（作成・読み込み・更新・削除）を行うAPIエンドポイントを実装。
3.  クライアント用 `.conf` テンプレート生成ロジックを実装。

### Phase 4: フロントエンドの実装
1.  ダッシュボード画面（概要カード・Peer サマリー・WireGuard バージョン）、ピア一覧（別ページ）、設定・ドキュメントを HTML/JS と組み込み CSS で実装済み。
2.  クライアント追加時、QR コードをモーダルで表示。.conf ダウンロード（PC 用）も実装。
3.  WireGuard サーバーの起動・停止ボタンは未実装。

### Phase 5: リファクタリングと配布パッケージ化 (PyInstaller)
1.  権限周りは **Worker** により対応済み（sudoers 不要）。手順は [docs/WORKER.md](docs/WORKER.md) および [docs/01-setup.md](docs/01-setup.md)。
2.  PyInstaller による単一バイナリ化（build.sh / .spec）は未整備。
3.  配布後の config 読み込みは `path_utils`（get_resource_path）で対応済み。

