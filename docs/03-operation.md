# 03 - 運用イメージ

日常の起動・停止の流れです。**Worker 利用を標準**とし、sudo 利用は代替として記載しています。

---

## 構成の二通り

1. **Worker 利用（推奨・標準）**  
   - `config.yaml` の `paths.wg_worker_socket` に **ソケットパスを指定**。  
   - Web アプリは一般ユーザーで起動し、wg 操作だけ Worker（root）にソケット経由で依頼する。**sudoers は不要**。  
   - 詳細は [WORKER.md](WORKER.md)。

2. **sudo 利用（非推奨）**  
   - `config.yaml` の `paths.wg_worker_socket` が **空**。  
   - Web アプリから `sudo wg ...` を実行するため、**sudoers でパスワードなし sudo を許可**する必要がある。

---

## 起動・停止（Worker 利用・標準, Rust 版）

### 起動順

1. **先に Worker を起動する**（root）
   ```bash
   sudo systemctl start wg-manager-worker
   # 常時起動にしている場合は enable 済みなら起動のみでよい
   sudo systemctl enable --now wg-manager-worker
   ```
2. **そのあと Web アプリを起動する**（一般ユーザー）
   - **Web アプリも systemd で動かしている場合（Rust 版）**
     ```bash
     # <webuser> は Web を動かす Unix ユーザー名（例: wgwm）
     sudo systemctl start wg-manager-web@<webuser>
     # または enable 済みなら
     sudo systemctl enable --now wg-manager-web@<webuser>
     ```
   - **手動で Rust バイナリを動かす場合**
     ```bash
     cd /path/to/wireguard-web-manager
     ./rust/target/release/wg-manager
     ```

### 停止

- **Web アプリ（systemd）** … `sudo systemctl stop wg-manager-web@<webuser>`。
- **Web アプリ（手動）** … 起動したターミナルで `Ctrl+C`。
- **Worker** … 必要なら `sudo systemctl stop wg-manager-worker`。

---

## 起動・停止（sudo 利用・非推奨, Rust 版）

- **Web アプリの起動のみ**（Worker は使わない）
  ```bash
  cd /path/to/wireguard-web-manager
  sudo CONFIG_PATH=/path/to/config.yaml ./rust/target/release/wg-manager
  ```
- **停止** … 起動したターミナルで `Ctrl+C`。

---

## 再起動

- **Web アプリだけ再起動する**  
  - systemd: `sudo systemctl restart wg-manager-web@<webuser>`  
  - 手動: 一度停止してから再度 `./rust/target/release/wg-manager` を実行する。
- **Worker を再起動する**  
  `sudo systemctl restart wg-manager-worker`。  
  Worker を再起動した場合は、必要に応じて Web アプリも再起動する（systemd なら `sudo systemctl restart wg-manager-web@<webuser>`）。

---

## ログ・状態確認

- **Web アプリ**  
  - systemd: `journalctl -u wg-manager-web -f` でログ、`systemctl status wg-manager-web` で状態確認。  
  - 手動: 標準出力にアクセスログやエラーが出る。
- **Worker**  
  systemd で動かしている場合:  
  `journalctl -u wg-manager-worker -f` でログを追う。  
  `systemctl status wg-manager-worker` で稼働状態を確認する。

- **クライアント有効/無効が切り替わらないとき**  
  Web アプリはエラーを syslog（および journald）に出力する。  
  - `journalctl -u wg-manager-web -n 100` で Web 側のエラー（Peer有効/無効の切り替え失敗など）を確認。  
  - `journalctl -u wg-manager-worker -n 100` で Worker 側の `wg set` 失敗メッセージを確認。  
  - 従来の syslog を使っている場合は `grep wg-manager /var/log/syslog` でも確認できる。

---

## 運用の流れ（まとめ）

| 項目 | Worker 利用（推奨） | sudo 利用 |
|------|---------------------|-----------|
| config の `wg_worker_socket` | ソケットパスを指定 | 空 `""` |
| sudoers | 不要 | 要設定 |
| 起動順 | 1. Worker → 2. Web アプリ（systemd なら `wg-manager-worker` / `wg-manager-web@<webuser>` を enable） | Web アプリのみ |
| 停止 | Web アプリ停止（systemd: stop）、必要なら Worker も stop | Web アプリを Ctrl+C |

設定の詳細は [02-config.md](02-config.md)、Worker の詳細は [WORKER.md](WORKER.md) を参照してください。マニュアル（本 docs）はログイン後の画面で「ドキュメント」からも参照できます（`/manual`）。
