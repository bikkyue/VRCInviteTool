# VRCInviteTool　[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

VRChatのAPIを使ってインスタンスの作成・フレンドへの招待を行うPythonツール。

個人的な利用のために作成したものとなります。
そのため、保守・サポートは期待しないでください。


## 動作環境 / 前提条件

- Windows 10 / 11
- 要 VRCアカウント

## インストール手順・実行方法

右のReleasesよりファイルをダウンロード・解凍をして.exeを起動してください。

## 機能一覧

- お気に入りワールドよりワールドを選択してインスタンスの作成
  - ワールドIDを直接指定も可能
- 指定をしたインスタンスへフレンドを招待
  - 複数人・自分自身にもインバイトを送信可能

## スクリーンショット

![](/docs/screenshot1-1.png)
![](/docs/screenshot1-2.png)



## 使い方

→ [HowToUse](/docs/HowToUse.md)


## 使用ライブラリ
このツールは以下のオープンソースソフトウェアを使用しています。

- [VRChat API Library](https://github.com/vrchatapi/vrchatapi-python) - Copyright (c) 2021 vrchatapi (MIT License)


## 免責事項

- 本ツールの使用によって生じたいかなるトラブル・損害（アカウントBANや利用規約違反を含む）についても、作者は一切の責任を負いません。

- 本ツールは [VRChat.community](https://vrchat.community/) 様が提供するVRChat非公式 のAPIのOSSを使用して作成しています。
- 以下に記載の通り、利用しているVRCのAPIについて扱いを理解をした上で利用をお願いいたします。

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
    - [VRChatクリエイターガイドライン](https://hello.vrchat.com/creator-guidelines)
  

- 本ツールは、ログイン処理のためにユーザーIDとパスワードを使用しますが、通信はすべてユーザーのローカル環境からAPIを介してVRChat公式サーバーへ行われます。開発者が認証情報を収集・保存することは一切ありません。なお、利便性のためユーザー名とセッション情報はお使いのPC上（`%APPDATA%\VRCInviteTool\`）にローカル保存されます。パスワードは保存されません。

- VRChatの利用規約を遵守した上で、**自己責任にて**ご使用ください。

#### LICENSE

- MIT LICENSE

    - MITライセンスとしておりますが、本ツールをそのまま（あるいは僅かな改変で）Booth等で公開・販売するのはご遠慮いただけますと幸いです。


