# Ondo - Hardware Temperature Monitor

アイアンマンのHUDをイメージした、Windows用ハードウェア温度モニタリングウィジェット。

## 機能

- CPU・GPU温度のリアルタイム表示
- 画面端へのドッキング表示
- 半透明でスタイリッシュなHUD風デザイン
- システムテーマ連動（ダーク/ライトモード自動切替）
- 起動時のブートシーケンスアニメーション
- システムトレイ常駐
- Windows起動時の自動起動（設定可能）

## スクリーンショット

起動時にアイアンマン風のブートシーケンスが表示され、その後メインウィジェットに移行します。

## 必要環境

- Windows 10/11
- Node.js 18以上
- Rust 1.70以上
- Tauri CLI

## セットアップ

```bash
# 依存関係のインストール
npm install

# 開発モードで起動
npm run tauri dev

# プロダクションビルド
npm run tauri build
```

## 設定

設定パネルから以下の項目を変更できます：

- **Position**: ウィジェットの表示位置（右端、左端、角など）
- **Opacity**: 透明度の調整（30-100%）
- **Always on Top**: 常に最前面に表示
- **Auto Start**: Windows起動時に自動起動
- **Show CPU Cores**: 各CPUコアの温度を表示
- **Update Interval**: 更新間隔（500ms-5000ms）
- **Theme**: テーマ（Auto/Dark/Light）

## 技術スタック

- **Frontend**: React 18 + TypeScript + Vite
- **Backend**: Tauri 2 (Rust)
- **Hardware Monitoring**: WMI (Windows Management Instrumentation)
- **Styling**: CSS with HUD-style animations

## 温度データについて

Windows標準のWMIを使用して温度データを取得しています。より詳細な温度情報が必要な場合は、LibreHardwareMonitorをインストールして実行してください。

## ライセンス

MIT License
