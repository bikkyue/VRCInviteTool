// Tauri コマンドの型付きラッパー。バックエンド (src-tauri/src/commands.rs) と 1:1 で対応する。
import { invoke } from "@tauri-apps/api/core";

export interface ApiError {
  kind: string;
  message: string;
  status?: number;
}

export function isApiError(e: unknown): e is ApiError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as ApiError).kind === "string" &&
    typeof (e as ApiError).message === "string"
  );
}

/** invoke の例外を ApiError に正規化する。 */
export function toApiError(e: unknown): ApiError {
  if (isApiError(e)) return e;
  if (e instanceof Error) return { kind: "unknown", message: e.message };
  return { kind: "unknown", message: String(e) };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toApiError(e);
  }
}

export interface AppInfo {
  version: string;
  dataDir: string;
  savedUsername: string | null;
}

export interface UserSummary {
  id: string;
  displayName: string;
}

export type LoginOutcome =
  | { status: "logged_in"; user: UserSummary }
  | { status: "two_factor_required"; methods: string[]; email: boolean }
  | { status: "not_logged_in"; message?: string };

export interface Friend {
  id: string;
  displayName: string;
  iconUrl?: string | null;
  status?: string | null;
  location?: string | null;
}

export interface User {
  id: string;
  displayName: string;
  iconUrl?: string | null;
  profilePicOverride?: string | null;
  profilePicOverrideThumbnail?: string | null;
  currentAvatarImageUrl?: string | null;
  currentAvatarThumbnailImageUrl?: string | null;
  isFriend: boolean;
}

export interface World {
  id: string;
  name: string;
  authorName?: string | null;
  thumbnailImageUrl?: string | null;
  imageUrl?: string | null;
}

/** お気に入り + 自作ワールドを結合した一覧の要素 (バックエンドで id 重複排除済み)。 */
export interface WorldEntry extends World {
  favorite: boolean;
  own: boolean;
  releaseStatus?: string | null;
  favoriteGroup?: string | null;
}

export interface Instance {
  id: string;
  worldId: string;
  instanceId: string;
  location: string;
  region?: string | null;
  type?: string | null;
}

export interface Notification {
  id: string;
  createdAt?: string | null;
  receiverUserId?: string | null;
}

export interface InviteTarget {
  id: string;
  name?: string;
}

export interface InviteResult {
  userId: string;
  ok: boolean;
  message?: string;
}

export interface InviteReport {
  results: InviteResult[];
  succeeded: number;
  failed: number;
  aborted: boolean;
}

export type UiInstanceType = "public" | "friends" | "hidden" | "invite" | "invite_plus";
export type Region = "jp" | "us" | "use" | "eu";

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  tryAutoLogin: () => call<LoginOutcome>("try_auto_login"),
  login: (username: string, password: string) => call<LoginOutcome>("login", { username, password }),
  submitTwoFactor: (code: string) => call<LoginOutcome>("submit_two_factor", { code }),
  cancelLogin: () => call<void>("cancel_login"),
  logout: () => call<void>("logout"),
  listWorlds: () => call<WorldEntry[]>("list_worlds"),
  getWorld: (worldId: string) => call<World>("get_world", { worldId }),
  createInstance: (worldId: string, instanceType: UiInstanceType, region: Region) =>
    call<Instance>("create_instance", { worldId, instanceType, region }),
  listFriends: () => call<Friend[]>("list_friends"),
  getUser: (userId: string) => call<User>("get_user", { userId }),
  inviteUsers: (location: string, targets: InviteTarget[]) =>
    call<InviteReport>("invite_users", { location, targets }),
  inviteSelf: (location: string) => call<Notification>("invite_self", { location }),
  fetchImage: (url: string) => call<string>("fetch_image", { url }),
};

/** ユーザーのアイコン URL (新旧フィールドの優先順)。 */
export function userIconUrl(u: User): string | null {
  return (
    u.iconUrl ||
    u.profilePicOverride ||
    u.profilePicOverrideThumbnail ||
    u.currentAvatarImageUrl ||
    u.currentAvatarThumbnailImageUrl ||
    null
  );
}

export function worldThumbnailUrl(w: World): string | null {
  return w.thumbnailImageUrl || w.imageUrl || null;
}
