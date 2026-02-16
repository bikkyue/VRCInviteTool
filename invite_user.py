"""
VRChatのユーザーをインスタンスに招待するスクリプト。
vrchatapi-python ライブラリを使用:
https://github.com/vrchatapi/vrchatapi-python
"""

from typing import Optional
import vrchatapi
from vrchatapi.api import invite_api
from vrchatapi.models import InviteRequest
from vrchatapi.exceptions import ApiException

def invite_myself(
    api_client: vrchatapi.ApiClient,
    location: str,
) -> bool:
    """
    自分自身をインスタンスに招待する。フレンド関係不要。

    Args:
        api_client: VRChat APIクライアント
        location: インスタンスのlocation文字列 (wrld_xxx:instanceId~... 形式)

    Returns:
        成功した場合 True、失敗した場合 False
    """
    if ":" not in location:
        print(f"\nエラー: location形式が不正です: {location}")
        return False

    world_id, instance_id = location.split(":", 1)

    inv_api = invite_api.InviteApi(api_client)

    print(f"\n自分への招待を送信中...")
    print(f"  ワールドID    : {world_id}")
    print(f"  インスタンスID: {instance_id}")

    try:
        notification = inv_api.invite_myself_to(world_id, instance_id)
        print(f"\n自分への招待を送信しました。")
        print(f"  通知ID   : {notification.id}")
        print(f"  送信日時 : {notification.created_at}")
        return True
    except ApiException as e:
        print(f"\nAPIエラー ({e.status}): {e.reason}")
        return False


def invite_user(
    api_client: vrchatapi.ApiClient,
    user_id: str,
    instance_id: str,
    message_slot: Optional[int] = None,
) -> bool:
    """
    指定したユーザーをインスタンスに招待する。

    Args:
        api_client: VRChat APIクライアント
        user_id: 招待するユーザーのID（例: usr_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx）
        instance_id: 招待先のインスタンスID（例: wrld_xxx~public(xxx)~region(jp)）
        message_slot: 招待メッセージのスロット番号（0〜11, 省略可）

    Returns:
        成功した場合 True、失敗した場合 False
    """
    inv_api = invite_api.InviteApi(api_client)

    request = InviteRequest(instance_id=instance_id)
    if message_slot is not None:
        request.message_slot = message_slot

    print(f"\n招待を送信中...")
    print(f"  ユーザーID   : {user_id}")
    print(f"  インスタンスID: {instance_id}")
    if message_slot is not None:
        print(f"  メッセージスロット: {message_slot}")

    try:
        notification = inv_api.invite_user(user_id, invite_request=request)
        print(f"\n招待を送信しました。")
        print(f"  通知ID   : {notification.id}")
        print(f"  送信先   : {notification.receiver_user_id}")
        print(f"  送信日時 : {notification.created_at}")
        return True
    except ApiException as e:
        if e.status == 403:
            print(f"\nエラー: ユーザー {user_id} はフレンドではないため招待できません。")
        else:
            print(f"\nAPIエラー ({e.status}): {e.reason}")
        return False
