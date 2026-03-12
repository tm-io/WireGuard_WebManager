# 02 - config.yaml カスタマイズ

人が変更する設定はすべて `config.yaml` に集約されています。

- **設定画面での編集:** ログイン後、左サイドバーの **「設定」** から各項目を編集・保存できます。パスワードは「変更する場合のみ入力」で空なら現状維持。保存後、host や port など一部の項目は Web アプリ／Worker の再起動後に反映されます。
- **直接編集:** プロジェクトルートの `config.yaml` をエディタで編集することもできます。

## 設定項目一覧

### app（Web アプリ・認証）

| 項目 | 説明 | 例・備考 |
|------|------|----------|
| **host** | バインドするアドレス | `"0.0.0.0"` = 全インターフェース（LAN からアクセス可）。`"127.0.0.1"` = ローカルのみ。 |
| **port** | 待ち受けポート | 他サービス（Zabbix 等）と重ならない番号にすること。例: `8080`。 |
| **auth_username** | ログイン用ユーザー名 | 管理画面の認証に使用。 |
| **auth_password** | ログイン用パスワード | 本番では推測困難な値に変更すること。 |

### paths（パス・Worker）

| 項目 | 説明 | 例・備考 |
|------|------|----------|
| **db_path** | SQLite の DB ファイルパス | 相対パスはプロジェクトルート基準。例: `"data/wg-manager.db"`。 |
| **wg_conf_dir** | WireGuard 設定が置かれるディレクトリ | 例: `"/etc/wireguard"`。 |
| **wg_worker_socket** | Worker の UNIX ソケットパス | **推奨:** パスを指定して Worker 経由（sudo 不要）。例: `"/var/run/wg-manager.sock"`。Worker 起動時にソケットと親ディレクトリは自動作成される。空 `""` にすると Web アプリから sudo で wg 実行（sudoers 要設定）。 |
| **socket_owner** | ソケットの所有者ユーザー名 | Worker がソケットを chown する先。Web アプリを動かすユーザー名を指定。例: `"wgwm"`。 |

### wireguard（WireGuard まわり）

| 項目 | 説明 | 例・備考 |
|------|------|----------|
| **interface** | 対象インターフェース名 | 例: `"wg0"`。`wg show wg0` で参照する名前と一致させる。 |
| **server_endpoint** | クライアント用 conf に書くサーバー宛先 | サーバーのグローバル IP または FQDN。例: `"203.0.113.1"` や `"vpn.example.com"`。 |
| **listen_port** | サーバーの WireGuard 待ち受け UDP ポート | 例: `51820`。 |
| **client_ip_range** | クライアントに割り当てる IP の範囲（CIDR） | 例: `"10.8.0.0/24"`。この範囲から未使用 IP を自動採番する。 |
| **client_dns** | クライアント用 conf に書く DNS | カンマ区切り。例: `"1.1.1.1, 8.8.8.8"`。 |
| **persistent_keepalive** | クライアント用 conf の KeepAlive 秒数 | NAT 越え用。例: `25`。 |

---

## 変更時の注意

- **app** の変更後は Web アプリの再起動が必要です。
- **paths** / **wireguard** の変更後も再起動が必要です。
- **wg_worker_socket** を空からパスに変える（または逆）した場合は、Worker の起動の有無と合わせて [03-operation.md](03-operation.md) の運用イメージを確認してください。
