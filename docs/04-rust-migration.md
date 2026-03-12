# 04 - Rust への移行方針

## 命令・要件（開発指示）

本プロジェクトは **Python から Rust へ完全にリファクタリング** する。以下を満たすこと。

1. **完全リファクタリング**  
   FastAPI 製の現行 Web アプリおよび Worker（`wg_worker.py`）を Rust で書き直し、同等機能を実現する。

2. **マルチ環境での動作**  
   **RHEL** および **Ubuntu** をはじめとする主要 Linux ディストリビューションで動作させる。  
   - 設定・パス・起動方法は `config.yaml` および systemd unit で統一し、ディストリに依存しない構成とする。  
   - 可能な範囲でスタティックリンクまたは依存ライブラリを最小化し、単一（または少数）バイナリで配布・実行できるようにする。

3. **ライブラリに依存せず動作**  
   - **実行時**に Python や多数のシステムライブラリに依存しない。Rust でビルドしたバイナリ単体（および `config.yaml`・データディレクトリ）で動作することを目標とする。  
   - **ビルド時**の Rust クレートは必要最小限に留める（HTTP サーバー・設定読み込み・DB・Worker プロトコル・QR・Markdown などに必要なもののみ）。  
   - 配布形態としては「スタティックリンクされた単一バイナリ（＋必要に応じて Worker 用バイナリ）」を想定し、各環境に Python や追加パッケージを入れずに動かせるようにする。

## 実装の配置

- Rust ソース・ビルド成果物: **`rust/`** ディレクトリに配置する。  
- ビルド手順・起動方法: 本ドキュメントおよび `rust/README.md` に記載する。  
- 移行完了後: Python 実装を削除し、Rust をルートの主実装とする方針でよい（その際は `docs/` の参照先を Rust 用に更新する）。

## 機能の対応関係

| 機能 | Python（現行） | Rust での対応 |
|------|----------------|----------------|
| 認証 | config の認証情報・Cookie セッション | 同等（Cookie ベース認証） |
| Peer 管理 | SQLite + Worker/sudo で wg 反映 | rusqlite + 同一 Worker プロトコル |
| 設定 | config.yaml 読み書き・設定画面 | serde_yaml で読み書き |
| Worker | UNIX ソケット・JSON 1 行 | 同一プロトコルで Rust 実装 |
| ドキュメント表示 | docs/*.md → Markdown 表示 | pulldown-cmark 等で HTML 化 |
| QR コード・.conf 出力 | qrcode/Pillow・テンプレート | 対応クレートで同等出力 |
| デプロイ | deploy/*.service | ExecStart を Rust バイナリに変更 |

## ビルド・実行（Rust）

```bash
cd rust
cargo build --release
# Web: ./target/release/wg-manager   (CONFIG_PATH 未指定時はカレントの config.yaml を参照)
# Worker: sudo ./target/release/wg-worker
```

systemd では `ExecStart` を上記バイナリに合わせて修正する。  
Rust 用の unit 例は `deploy/rust-wg-manager-web.service.example` および `deploy/rust-wg-manager-worker.service.example` を参照し、パスを環境に合わせて編集してから `/etc/systemd/system/` に配置する。

---

本命令は、Rust 移行の設計・実装を行う際の必須要件として参照すること。
