"""
VRChatのインスタンスを作成するモジュール。
vrchatapi-python ライブラリを使用:
https://github.com/vrchatapi/vrchatapi-python
"""

import vrchatapi
from vrchatapi.api import instances_api, authentication_api
from vrchatapi.models import CreateInstanceRequest


def create_instance(
    api_client: vrchatapi.ApiClient,
    world_id=None,
    instance_type="public",
    instance_region="jp",
) -> str:
    """インスタンスを作成して情報を表示し、location文字列を返す。"""

    _TYPE_DISPLAY = {
        "public": "Public",
        "friends": "Friends",
        "hidden": "Friends+",
        "invite": "Invite",
        "invite_plus": "Invite+",
    }
    display_type = _TYPE_DISPLAY.get(instance_type, instance_type)

    inst_api = instances_api.InstancesApi(api_client)

    # invite / invite+ を API の private にマッピング
    can_request_invite = False
    if instance_type == "invite_plus":
        instance_type = "private"
        can_request_invite = True
    elif instance_type == "invite":
        instance_type = "private"

    # owner_id の決定 (public 以外は自分のユーザーID)
    owner_id = None
    if instance_type != "public":
        auth_api = authentication_api.AuthenticationApi(api_client)
        current_user = auth_api.get_current_user()
        owner_id = current_user.id

    request = CreateInstanceRequest(
        world_id=world_id,
        type=instance_type,
        region=instance_region,
        owner_id=owner_id or None,
        can_request_invite=True if can_request_invite else None,
        hard_close=None,
        invite_only=None,
        queue_enabled=None,
        age_gate=None,
    )

    print(f"\nインスタンスを作成中...")
    print(f"  ワールドID : {world_id}")
    print(f"  タイプ     : {display_type}")
    print(f"  リージョン : {instance_region}")

    instance = inst_api.create_instance(request)

    print("\n--- 作成されたインスタンス ---")
    print(f"  インスタンスID : {instance.id}")
    print(f"  ワールドID     : {instance.world_id}")
    print(f"  タイプ         : {display_type}")
    print(f"  リージョン     : {instance.region}")
    print(f"  場所           : {instance.location}")

    return instance.location
