# WireGuard Web Manager - Worker による権限分離（推奨構成）

本番では **Worker 利用を推奨** します。Web アプリは **一般ユーザー（例: kanri）** で動作させ、`wg` など特権が必要な操作だけ **root で動く Worker** に UNIX ソケット経由で依頼する構成です。sudoers に NOPASSWD を書く必要がなく、セキュリティリスクを抑えられます。

## 仕組み

1. **Worker**（`wg_worker.py`）を **root** で systemd サービスとして起動する。
2. Worker はローカルの **UNIX ドメインソケット**（例: `/var/run/wg-manager.sock`）をリッスンする。
3. 起動後にソケットファイルを **Web アプリを動かすユーザー（例: kanri）** に chown し、パーミッションを `660` にする。
4. Web アプリ（FastAPI）は、ソケットに接続して「公開鍵取得」「Peer 一覧（dump）」「Peer 追加/削除」などの **決められた JSON メッセージだけ** を送る。
5. Worker はそのメッセージに応じて `wg` コマンド（sudo なし）を実行し、結果を JSON で返す。

**メリット**

- Web アプリが乗っ取られても、実行できるのは Worker が実装した WireGuard 操作のみに限定される。
- sudoers を触らずに、root 権限が必要な処理だけを Worker に集約できる。

## 設定手順

### 1. config.yaml で Worker 用のパスを指定する

```yaml
paths:
  db_path: "data/wg-manager.db"
  wg_conf_dir: "/etc/wireguard"
  wg_worker_socket: "/var/run/wg-manager.sock"   # 推奨。空にすると sudo 利用（sudoers 要設定）
  socket_owner: "kanri"   # ソケットの所有者（Web アプリを動かすユーザー名）
```

- **wg_worker_socket** にソケットパスを指定すると、Web アプリは **Worker 経由のみ** で wg 操作を行い、sudo は使いません（推奨）。
- ソケットファイルとその親ディレクトリは **Worker 起動時に自動作成** されます。例: `/var/run/wg-manager.sock` の親 `/var/run` は通常既に存在しますが、`/var/run/wg-manager/socket` のようにした場合は `/var/run/wg-manager` が無ければ作成されます。
- 空 `""` にすると、Web アプリから `sudo wg` を実行する動作になり、sudoers 設定が必要です。

### 2. Worker を root で起動する（systemd）

- `deploy/wg-manager-worker.service` を `/etc/systemd/system/` にコピーし、`WorkingDirectory` と `ExecStart` のパスを環境に合わせて修正する。
- 起動前に `config.yaml` の `paths.wg_worker_socket` と `paths.socket_owner` を設定しておく。

```bash
sudo cp deploy/wg-manager-worker.service /etc/systemd/system/
# 必要に応じて編集
sudo systemctl daemon-reload
sudo systemctl enable --now wg-manager-worker
sudo systemctl status wg-manager-worker
```

- Worker は起動時にソケットを作成し、`socket_owner` のユーザーに chown するため、**先に Worker を起動してから** Web アプリを起動する。

### 3. Web アプリの起動（一般ユーザー）

- **systemd で起動する場合:** [01-setup.md](01-setup.md) の「5. Web アプリを起動する」を参照。`deploy/wg-manager-web.service` を `/etc/systemd/system/` に配置し、`systemctl enable --now wg-manager-web` で起動する。
- **手動で起動する場合:**
  ```bash
  cd /home/kanri/wireguard-web-manager
  source .venv/bin/activate
  python -m app.main
  ```
- `config.yaml` で `wg_worker_socket` を設定している場合、Web アプリはそのソケットに接続して wg 操作を行う。sudo は不要。

## Worker が受け付けるコマンド（JSON）

| cmd | 説明 | パラメータ |
|-----|------|------------|
| `get_public_key` | サーバー公開鍵取得 | なし |
| `get_peer_stats` | `wg show <interface> dump` の結果をパースして返す | なし |
| `peer_set` | Peer 追加/更新 | `public_key`, `allowed_ips`（配列）, `preshared_key`（任意） |
| `peer_remove` | Peer 削除 | `public_key` |

いずれも 1 行 1 リクエスト・1 行 1 レスポンスの JSON（改行区切り）。応答は `{"ok": true, ...}` または `{"ok": false, "error": "..."}`。

## トラブルシュート

- **「Worker に接続できません」**  
  - Worker が起動しているか `systemctl status wg-manager-worker` で確認する。  
  - ソケットパスが config と一致しているか、Web アプリを動かすユーザーがソケットに読み書きできるか（`ls -la /var/run/wg-manager.sock`）を確認する。

- **Worker が「User not found」**  
  - `config.yaml` の `paths.socket_owner` が実在するユーザー名になっているか確認する。
