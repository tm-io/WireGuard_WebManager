# 01 - 初回セットアップ

WireGuard Web Manager をサーバーに初めて導入するときの手順です。**wg 操作は Worker 経由（推奨）** で行う前提で説明しています。

## 前提

- Linux（Ubuntu 想定）で WireGuard がインストールされ、`wg0` などインターフェースが利用可能であること。
- Python 3.10 以上が利用できること。

## 手順

### 1. プロジェクトを配置する

- ソースを任意のディレクトリに配置する（例: `/home/kanri/wireguard-web-manager`）。
- 以降、このディレクトリを「プロジェクトルート」と呼びます。

### 2. 仮想環境を作成し、依存関係を入れる

```bash
cd /home/kanri/wireguard-web-manager
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

### 3. 設定ファイルを編集する

- プロジェクトルートの `config.yaml` を環境に合わせて編集する。
- **Worker 利用（推奨）** のため、次を設定する。
  - **paths.wg_worker_socket** … Worker のソケットパス（例: `"/var/run/wg-manager.sock"`）。初期値でこのパスが入っていればそのままでよい。
  - **paths.socket_owner** … Web アプリを動かすユーザー名（例: `"kanri"`）。
- あわせて次も確認・変更する。
  - **app.port** … 他サービス（Zabbix 等）と重ならないポート。
  - **app.auth_username** / **app.auth_password** … ログイン用の認証情報。
  - **wireguard.interface** … 利用する WireGuard インターフェース名（例: `wg0`）。
  - **wireguard.server_endpoint** … クライアント設定に書くサーバーの IP または FQDN。

設定項目の詳細は [02-config.md](02-config.md) を参照してください。

### 4. Worker を root で起動する（systemd）

- Worker が **先に** 起動している必要があります。Web アプリは Worker のソケットに接続して wg 操作を行います。

```bash
# ユニットファイルをコピーし、WorkingDirectory / ExecStart のパスを環境に合わせて編集する
sudo cp deploy/wg-manager-worker.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wg-manager-worker
sudo systemctl status wg-manager-worker
```

- パスの編集例は [WORKER.md](WORKER.md) を参照してください。

### 5. Web アプリを起動する

**方法 A: systemd で起動する（推奨・常時運用向け）**

```bash
sudo cp deploy/wg-manager-web.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wg-manager-web
sudo systemctl status wg-manager-web
```

- Web アプリは一般ユーザー（例: `kanri`）で動作し、Worker の**後**に自動起動します。プロジェクトパスは unit 内の `WorkingDirectory` / `ExecStart` を環境に合わせて編集してください。

**方法 B: 手動で起動する**

```bash
cd /home/kanri/wireguard-web-manager
source .venv/bin/activate
python -m app.main
```

- ブラウザで `http://<サーバーIP>:<app.port>/login` にアクセスする。
- `config.yaml` の `app.auth_username` / `app.auth_password` でログインする。

### 6. ログイン後の画面

ログインするとダッシュボードが表示されます。左サイドバーから次のページへ移動できます。

- **ダッシュボード** … サーバー状態・Peer 数・WireGuard バージョンなどの概要。
- **ピア一覧** … クライアントの追加・有効/無効・.conf ダウンロード・QRコード・削除。
- **設定** … config.yaml の項目を編集・保存（一部は再起動後に反映）。
- **ドキュメント** … 本マニュアル（docs フォルダ）をブラウザで参照。

---

## sudo で wg を実行する場合（非推奨）

Worker を使わず、Web アプリから `sudo wg` で実行する場合は、`config.yaml` の **paths.wg_worker_socket** を **空 `""`** にし、sudoers でパスワードなしの `wg` 実行を許可する必要があります。運用は [03-operation.md](03-operation.md) の「sudo 利用」を参照してください。セキュリティ上、Worker 利用を推奨します。

---

初回セットアップ後は、[03-operation.md](03-operation.md) の運用イメージに従って起動・停止を行います。
