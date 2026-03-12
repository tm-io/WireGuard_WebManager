# WireGuard Web Manager 運用・マニュアル

本ディレクトリには、導入から日常運用・カスタマイズまでをまとめたマニュアルを格納しています。**wg 操作は Worker 利用（推奨）** を前提に記載しています。

**アプリ内での参照:** ログイン後、サイドバーの「ドキュメント」をクリックすると、本マニュアルをブラウザで参照できます（URL: `/manual`）。

| ドキュメント | 内容 |
|-------------|------|
| [01-setup.md](01-setup.md) | 初回セットアップ（環境構築・Worker 設定・Web/Worker の systemd 化・起動までの手順） |
| [02-config.md](02-config.md) | config.yaml のカスタマイズ（設定項目の一覧。画面の「設定」からも編集可能） |
| [03-operation.md](03-operation.md) | 運用イメージ（起動・停止・再起動・ログ確認。Worker 利用を標準、sudo は代替） |
| [WORKER.md](WORKER.md) | Worker による権限分離（構成・設定・トラブルシュート） |

---

- **初めて導入する** → [01-setup.md](01-setup.md) から順に実施（Worker を設定してから Web アプリを起動）。
- **ポートや認証・WireGuard の値を変えたい** → [02-config.md](02-config.md)。画面の「設定」からも変更可能。
- **普段の起動・再起動の手順を確認したい** → [03-operation.md](03-operation.md)。
- **Worker の仕組みやトラブルシュート** → [WORKER.md](WORKER.md)。
