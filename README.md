# VRCInviteTool　[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

VRChat の API を使ってインスタンスの作成・フレンドへの招待を行う Windows 用デスクトップツールです。
**Rust + Tauri 2** で実装されており、単体の `.exe` として配布されます (以前の Python/Flet 版は [`legacy-python/`](/legacy-python/) に参照用として残しています)。

個人的な利用のために作成したものとなります。
そのため、保守・サポートは期待しないでください。


## 動作環境 / 前提条件

- Windows 10 / 11 (64bit)
- Microsoft Edge WebView2 ランタイム (Windows 11 には標準搭載。無い場合はインストーラ版が自動でダウンロードします。ポータブル版 `.exe` は事前にインストールされている必要があります)
- 要 VRChat アカウント

## インストール手順・実行方法

右の Releases から以下のいずれかをダウンロードしてください。

- `VRCInviteTool-vX.Y.Z-windows-x64.zip` — 解凍して `VRCInviteTool.exe` を起動するだけのポータブル版
- `VRCInviteTool_X.Y.Z_x64-setup.exe` — インストーラ版 (現在のユーザーのみにインストール)

## 機能一覧

- お気に入りワールド・自作ワールド (非公開含む) から選択してインスタンスの作成
  - ワールド ID を直接指定も可能
  - タイプ: Public / Friends / Friends+ / Invite / Invite+、リージョン: jp / us / use / eu
- 指定したインスタンスへフレンドを招待
  - フレンド名の一部や `usr_...` ID を入力してフォーカスを外す (または Enter) だけでも選択できます
  - 複数人 (最大 20 人)・自分自身にもインバイトを送信可能
- ログイン状態の保存 (次回起動時に自動ログイン)、二段階認証 (メール / 認証アプリ / リカバリーコード) 対応

## スクリーンショット

<img src="/docs/screenshot-main.png" alt="VRCInviteTool のメイン画面 (ログイン後、インスタンス作成済み)" width="600">

## 使い方

→ [HowToUse](/docs/HowToUse.md)

## データの保存場所

`%APPDATA%\VRCInviteTool\` (通常は `C:\Users\<ユーザー名>\AppData\Roaming\VRCInviteTool\`) に以下を保存します。

| ファイル | 内容 |
|---|---|
| `session.bin` | ログインセッション (Cookie)。Windows の DPAPI で **現在の Windows ユーザーのみ復号できる形に暗号化** して保存します |
| `config.json` | 最後にログインしたユーザー名 (平文) |
| `logs\vrcinvitetool.log.YYYY-MM-DD` | 動作ログ (日別)。パスワード・Cookie の値は記録しません |

パスワードは保存されません (ログイン後はメモリからも破棄し、以降は Cookie のみで認証します)。
「ログアウト」ボタンを押すとサーバー側のセッションを無効化し、`session.bin` を削除します。

## ソースからビルドする

必要なもの: [Rust](https://rustup.rs/) (stable)、Node.js 22 以上、Windows では Visual Studio Build Tools (C++ ワークロード) と WebView2。
詳細は [Tauri の前提条件](https://v2.tauri.app/start/prerequisites/) を参照してください。

```powershell
npm ci
npm run tauri build
```

- 単体の実行ファイル: `src-tauri\target\release\VRCInviteTool.exe`
- インストーラ: `src-tauri\target\release\bundle\nsis\VRCInviteTool_X.Y.Z_x64-setup.exe`

開発時は `npm run tauri dev` でホットリロード付きで起動できます。

### 構成

```
src/                 フロントエンド (Vite + TypeScript、フレームワーク無し)
src-tauri/           Tauri アプリ本体 (Rust)。Tauri コマンドとアプリ状態のみ
src-tauri/core/      VRChat API クライアント・セッション保存 (Tauri 非依存の Rust クレート。`cargo test` はここが中心)
legacy-python/       旧 Python/Flet 実装 (参照用)
docs/                使い方
```

バージョン番号は `src-tauri/Cargo.toml` の `version` が唯一の情報源です (User-Agent、インストーラ名、`tauri.conf.json` はここから取得されます)。

### テスト

```bash
cd src-tauri
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd .. && npm run typecheck
```

`src-tauri/core` は Tauri に依存しないため Linux/macOS でもテストできます。GitHub Actions では Linux で lint/test、Windows で `.exe` とインストーラのビルドを行い、`v*` タグを push するとドラフトの Release が作成されます。

## 使用ライブラリ

このツールは以下のオープンソースソフトウェアを使用しています (いずれも MIT / Apache-2.0 等の許諾ライセンス)。

- [Tauri](https://tauri.app/) (MIT / Apache-2.0) — デスクトップアプリフレームワーク
- [reqwest](https://github.com/seanmonstar/reqwest) (MIT / Apache-2.0)、[native-tls](https://github.com/sfackler/rust-native-tls) (MIT / Apache-2.0) — HTTP / TLS (TLS は OS 標準の実装 = Windows では SChannel を使用)
- [cookie_store](https://github.com/pfernie/cookie_store) / [reqwest_cookie_store](https://github.com/pfernie/reqwest_cookie_store) (MIT / Apache-2.0) — Cookie 管理
- [serde](https://serde.rs/) (MIT / Apache-2.0)、[tokio](https://tokio.rs/) (MIT)、[tracing](https://github.com/tokio-rs/tracing) (MIT)、[regex](https://github.com/rust-lang/regex) (MIT / Apache-2.0)、[windows-sys](https://github.com/microsoft/windows-rs) (MIT / Apache-2.0) ほか
- [Vite](https://vitejs.dev/) (MIT)、[TypeScript](https://www.typescriptlang.org/) (Apache-2.0)
- アイコンは [Material Icons](https://fonts.google.com/icons) (Apache-2.0) のパスデータを使用
- API の仕様は [VRChat API Documentation](https://vrchatapi.github.io/) (コミュニティによる非公式ドキュメント) を参考にしています

## 免責事項

- 本ツールの使用によって生じたいかなるトラブル・損害 (アカウント BAN や利用規約違反を含む) についても、作者は一切の責任を負いません。

- 本ツールは [VRChat.community](https://vrchat.community/) 様が公開している VRChat 非公式 API のドキュメントに基づいて作成しています。
- 以下に記載の通り、利用している VRChat の API について扱いを理解した上で利用をお願いいたします。

    ```
    (VRChat.community様より引用)
    VRChat's API is not officially supported or documented by VRChat.
    This documentation project is maintained on a best-effort basis by the community and attempts to smooth over API breakage by quickly updating when endpoints change.
    Use responsibly and be aware that endpoints may still break without notice. Abuse of the API may result in account termination.
    For their official stance, refer to
    VRChat's Creator Guidelines.

    VRChatのAPIはVRChatによって公式にサポートされておらず、ドキュメントも作成されていません。
    このドキュメント作成プロジェクトはコミュニティによってベストエフォートベースで維持管理されており、
    エンドポイントの変更時に迅速に更新することでAPIの不具合を解消することを目的としています。
    責任ある利用を心がけてください。エンドポイントは予告なく機能しなくなる可能性がありますので、
    ご注意ください。APIの不正使用はアカウント停止につながる可能性があります。公式見解については、
    VRChatのクリエイターガイドラインをご覧ください。
    ```
    - [VRChat クリエイターガイドライン](https://hello.vrchat.com/creator-guidelines)

- 本ツールは、ログイン処理のためにユーザー名とパスワードを使用しますが、通信はすべてユーザーのローカル環境から VRChat 公式サーバー (`api.vrchat.cloud`) へ直接行われます。開発者が認証情報を収集・保存することは一切ありません。なお、利便性のためユーザー名とセッション情報はお使いの PC 上 (`%APPDATA%\VRCInviteTool\`、セッションは DPAPI で暗号化) にローカル保存されます。パスワードは保存されません。

- VRChat の利用規約を遵守した上で、**自己責任にて**ご使用ください。

#### LICENSE

- MIT LICENSE

    - MIT ライセンスとしておりますが、本ツールをそのまま (あるいは僅かな改変で) Booth 等で公開・販売するのはご遠慮いただけますと幸いです。
