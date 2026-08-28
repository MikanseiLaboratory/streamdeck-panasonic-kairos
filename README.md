# Stream Deck plugin for Panasonic KAIROS

Elgato Stream DeckからPanasonic KAIROSをREST APIで操作するプラグインです。

このプロジェクトはPanasonic非公式です。KAIROS本体のREST API（既定 `192.168.10.10:1234`、ユーザー名 `Kairos`）に接続します。

## Download

[最新リリース](https://github.com/MikanseiLaboratory/streamdeck-panasonic-kairos/releases/latest)から`.streamDeckPlugin`をダウンロードし、ダブルクリックしてStream Deckにインストールしてください。

## 動作

- Play Macro / Recall Snapshot / Scene Action / Cut / Auto / AUX Source / Layer Source / Multiviewer Preset をキーに割り当てられます。
- 候補リスト（macros、scenes など）は実機への GET から取得し、プロファイルの settings には UUID だけを保存します。
- Layer Source は sourceA 一致で赤、sourceB 一致で緑に点灯します。AUX Source は現在の source 一致で赤です。
- 同じ `(host, port, password, https)` のキーは HTTP 接続を共有します。
- 切断時は 1s → 2s → 4s …（上限 30s）で再接続します。
- キーが画面から消えてもすぐには切断せず、約 30 秒のアイドル後に接続を閉じます。
