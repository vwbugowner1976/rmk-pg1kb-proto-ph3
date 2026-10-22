# PG1KB Proto PH3 — RMK Firmware

PG1KB Proto PH3 を [RMK 0.9](https://github.com/HaoboGu/rmk) で動かすための実験・実機検証用ファームウェアです。Seeed Studio XIAO nRF52840 Plus を左右に搭載し、キー入力、BLE split、PAW3222 トラックボール、USB/BLE HID、MyKeebStudio からのランタイム設定を統合しています。

> **現在の開発ブランチ:** `main-rmk`  
> この README は `main-rmk` の実装に合わせています。実装済みの機能と、今後の検証・開発項目を区別して記載しています。

## 現在の構成

- **ファームウェア:** RMK 0.9 / Rust
- **MCU:** 左右それぞれ Seeed Studio XIAO nRF52840 Plus
- **分割接続:** BLE split
- **Central:** 右側
- **Peripheral:** 左側
- **キー配列:** RMK 内部の 5 行 × 12 列マトリクス（左右各 5 × 6 キー相当）
- **ポインティングセンサー:** 左右それぞれ PAW3222
- **設定ツール:** [MyKeebStudio](https://github.com/mykeebstudio/mykeebstudio.github.io) の RMK / Rynk ページ（WebHID）
- **ビルド:** WSL2 / Ubuntu でローカルビルド

## 実装済みの機能

### キー入力・レイヤー

- Base / Num / Sym / Sys を含む 8 レイヤー構成
- Num / Sym 用のレイヤータップ動作
- 他キーの押下時にレイヤータップをホールド側へ確定する設定
- 日本語配列を US 配列ホスト上で扱うためのキー変換設定
- BLE split による左右接続

### PAW3222 トラックボール

- PAW3222 の検出・初期化と Product ID `0x30` の確認
- 12-bit X/Y モーションデータの取得
- CPI 設定
- 左右それぞれのトラックボール入力
- Cursor / Scroll モード
- レイヤーごと・左右デバイスごとの設定プロファイル
- カーソル速度、スクロール速度、回転、慣性などのランタイム設定
- USB/BLE のアクティブな HID 経路を通じたマウスレポート送信
- スクロール方向のノイズ・反転しきい値処理

トラックボールの回転はデバイス別・レイヤー別の有効設定を使います。MyKeebStudio のレイヤープロファイル設定は、共通のベース設定より優先されます。

### MyKeebStudio / Rynk ランタイム設定

`keyboard.toml` を再ビルドせず、接続中に MyKeebStudio から設定を読み書きするための Rynk 拡張を含みます。

- ライブキー配列の読み取り・編集
- トラックボールの動作設定、レイヤープロファイルの読み書き
- 設定の保存・既定値の読み込み
- Runtime Combo の読み書き・保存
- ランタイム設定を扱うためのファームウェア側カスタムフック

**Combo は静的な `[[combo]]` 定義ではなく、MyKeebStudio から設定する Runtime Combo を使用します。** RMK 0.9 / Rynk の Combo wire format を維持しながら、PG1KB のファームウェアパッチで物理マトリクス位置をトリガーとして扱います。そのため、設定画面で指定した物理キー位置を使った Combo を実行時に設定できます。

### バッテリー

- 左右それぞれのバッテリー電圧を ADC で取得する構成
- PG1KB の非 LiPo 電池向け設定

取得値の精度や左右間の通知を含め、実機での検証が必要な項目は、確定済みの機能として扱わず引き続き確認します。バッテリー履歴ログはこの `main-rmk` ブランチの README では対象外です。

## ビルド

開発環境の準備、WSL2 / Ubuntu の依存パッケージ、Rust のセットアップについては [`docs/LOCAL_BUILD.md`](docs/LOCAL_BUILD.md) を参照してください。

### ビルドコマンド

WSL2 / Ubuntu 上でリポジトリのルートに移動して実行します。

```bash
cd ~/rmk-dev/rmk-pg1kb-proto-ph3
bash build-v7.sh
```

`build-v7.sh` は、Cargo registry 内の RMK 0.9.0 と rmk-macro 0.8.0 をプロジェクト内の `.rmk-patched/` にコピーし、PG1KB 用パッチを適用してから UF2 をビルドします。グローバルな Cargo registry のソースを直接変更しない構成です。

ビルド時には主に次のパッチが適用されます。

- Central の BLE advertising 設定
- split trackball のレイテンシ設定
- 物理キー位置を使う Runtime Combo 対応
- Rynk カスタムコマンド
- PG1KB のランタイム設定・永続化処理
- 非 LiPo バッテリー対応

ビルド後、UF2 は `/mnt/d/rmk-firmware/` にコピーされます。

- `D:\rmk-firmware\rmk-pg1kb-proto-ph3-central-v7.uf2`
- `D:\rmk-firmware\rmk-pg1kb-proto-ph3-peripheral-v7.uf2`

### 手動ビルドについて

このプロジェクトでは PG1KB 用の RMK パッチを適用する必要があるため、通常は上記の `bash build-v7.sh` を使用してください。パッチを適用しない通常の `cargo make uf2 --release` だけでは、Runtime Combo や Rynk の PG1KB 拡張を含まないファームウェアになる可能性があります。

## PAW3222 配線

左右とも同じ信号割り当てです。

| 信号 | nRF52840 pin | XIAO nRF52840 Plus |
|---|---|---|
| SCLK | P1.05 | D18 |
| SDIO | P1.07 | D19 |
| CS | P1.03 | D17 |
| MOTION | P1.15 | D10 |

## 主要ファイル

| パス | 役割 |
|---|---|
| `keyboard.toml` | キーボード、キー配列、レイヤー、BLE、Rynk 設定 |
| `build-v7.sh` | PG1KB パッチ適用と Central / Peripheral の UF2 ビルド |
| `src/paw3222.rs` | PAW3222 ドライバと入力レポート処理 |
| `src/split_pointing_ble.rs` | split 側トラックボールのカーソル・スクロール処理 |
| `src/runtime.rs` | トラックボールのランタイム設定と有効プロファイル |
| `scripts/patch_rmk_position_combos.sh` | 物理位置をトリガーにする Runtime Combo パッチ |
| `scripts/patch_rmk_rynk_custom_hook.sh` | Rynk の PG1KB カスタムコマンド拡張 |
| `scripts/patch_rmk_pg1kb_runtime.sh` | ランタイム設定・ストレージ・HID 経路などの RMK 拡張 |
| `scripts/patch_rmk_non_lipo_battery.sh` | 非 LiPo バッテリー対応パッチ |
| `docs/LOCAL_BUILD.md` | WSL2 / Ubuntu のビルド環境構築手順 |

## 現在の制約・確認事項

- バッテリー残量の取得値が実測値として正確か、校正を含めた確認が必要です。
- 左右間でのバッテリー情報通知・同期は、実機での動作確認が必要です。
- トラックボールの設定や HID 動作は、接続方式（USB / BLE）と左右の組み合わせで継続して実機検証します。
- RMK 本体に対する変更はビルドスクリプト内のパッチで適用しています。RMK のバージョンやソース構造が変わった場合は、パッチの適用結果を確認する必要があります。

## 関連プロジェクト

- [MyKeebStudio](https://github.com/mykeebstudio/mykeebstudio.github.io) — RMK / Rynk のキー配列・トラックボール・Runtime Combo 設定 UI
- [RMK](https://github.com/HaoboGu/rmk) — Rust ベースのキーボードファームウェアフレームワーク

## バッテリー履歴

MyKeebStudio の RMK Trackball 画面では、ファームウェアに保存したバッテリー残量履歴を `Refresh history` から読み出せます。ファームウェアは30分ごとに残量・充電状態・起動後の経過時間を記録し、最大40件（約20時間分）をリングバッファに保持します。記録はキーボードのフラッシュに保存され、履歴のクリアもMyKeebStudioから実行できます。

この履歴はバッテリー残量の記録であり、電流や実容量を直接測定するものではありません。現時点の実装はRMK側の `BatteryStatusEvent` を記録するため、左右それぞれの電池が別々のイベントとして届くかは実機で確認が必要です。
