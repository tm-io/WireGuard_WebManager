# WireGuard_WebManager PROGRESS.md

## 概要
Rust製WireGuard VPN管理WebUIデーモン。`.deb`/`.rpm`パッケージでsystemdサービスとして配布。

---

## 作業ログ

### 2026-03-21

#### 対応内容
以下の4機能を実装した。

**1. ピアの接続状態リアルタイム表示**
- `api_server_peer_stats` の `connected` 判定を「`latest_handshake` が直近180秒以内」に修正
- `peers.html` のピア行に `.conn-dot` インジケーター（緑=接続中 / グレー=切断）を追加
- `fetchPeerStats` (5秒ポーリング) でインジケーターを更新

**2. トラフィック統計の時系列保存**
- `db.rs` に `peer_traffic_log` テーブル・`TrafficSnapshot` 構造体・`record_traffic_snapshot` / `get_traffic_history` / `prune_traffic_log` メソッドを追加
- `main.rs` に 5 分ごとに wg stats を DB へ保存するバックグラウンドタスク (`traffic_recording_task`) を追加
- `GET /api/peers/:id/traffic` エンドポイントを追加（最新48件など取得可）
- `peers.html` のピアメニューに「トラフィック履歴」を追加（モーダルでテーブル表示）

**3. ピア名の変更（編集機能）**
- `db.rs` に `update_peer_name` メソッドを追加
- `UpdatePeerReq` を `is_active: Option<bool>`, `name: Option<String>` に変更（既存の toggle は後方互換を維持）
- `peers.html` のピアメニューに「名前変更」を追加（`window.prompt` ベース）

**4. バックアップ・リストア**
- `db.rs` に `pub fn path()` ゲッターを追加
- `GET /api/backup/db` — DB ファイルをダウンロード
- `POST /api/backup/restore` — SQLite マジックバイト検証 + integrity_check 後にDB置換（既存は `.bak` へ退避）
- `settings.html` にバックアップ/リストアセクションを追加

#### 解決した問題
- 接続状態が「handshakeが存在すれば接続中」になっていた → 180秒タイムアウトで正確に判定
- ピア名が作成後に変更できなかった

#### 次のTODO
- パスワードのハッシュ保存（現状config.yamlに平文）
- RPM系でのwgアップデート対応（dnf未対応）
- トラフィック履歴のグラフ表示（Chart.js等）
