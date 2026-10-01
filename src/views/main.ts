// メイン画面: インスタンス作成 / 招待 / ログ の 3 セクション。
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  api,
  type ApiError,
  type Friend,
  type Region,
  type UiInstanceType,
  type UserSummary,
  type WorldEntry,
  userIconUrl,
  worldThumbnailUrl,
} from "../api";
import { logStore } from "../log";
import { banner, card, debounce, el, icon, sectionHeader, show, spinner } from "../ui";

const MAX_SELECTED_FRIENDS = 20;
/** 連打防止: アクションボタンは最短でも 2 秒間無効にする (Python 版と同じ)。 */
const BUTTON_LOCK_MS = 2000;
const DROPDOWN_LIMIT = 20;

const INSTANCE_TYPES: { value: UiInstanceType; label: string }[] = [
  { value: "public", label: "Public" },
  { value: "friends", label: "Friends" },
  { value: "hidden", label: "Friends+" },
  { value: "invite", label: "Invite" },
  { value: "invite_plus", label: "Invite+" },
];
const REGIONS: Region[] = ["jp", "us", "use", "eu"];
const USER_ID_RE = /^usr_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export interface MainViewOptions {
  user: UserSummary;
  onLogout: () => void;
  onSessionExpired: () => void;
}

export interface MainView {
  root: HTMLElement;
  refreshData(): Promise<void>;
}

export function createMainView(opts: MainViewOptions): MainView {
  let friends: Friend[] = [];
  let worlds: WorldEntry[] = [];
  let sessionExpired = false;

  // ---------------------------------------------------------------- 共通

  /** エラー処理。401 ならログイン画面へ戻す (true を返す)。それ以外はログに出す。 */
  function handleError(e: unknown, prefix = "エラー"): boolean {
    const err = e as ApiError;
    if (err && err.kind === "unauthorized") {
      if (!sessionExpired) {
        sessionExpired = true;
        opts.onSessionExpired();
      }
      return true;
    }
    logStore.append(`${prefix}: ${err?.message ?? String(e)}`);
    return false;
  }

  const actionButtons: HTMLButtonElement[] = [];
  let unlockAt = 0;
  let unlockTimer: ReturnType<typeof setTimeout> | undefined;
  function setButtonsDisabled(disabled: boolean): void {
    if (disabled) {
      unlockAt = Date.now() + BUTTON_LOCK_MS;
      if (unlockTimer) clearTimeout(unlockTimer);
      for (const b of actionButtons) b.disabled = true;
      return;
    }
    const remaining = unlockAt - Date.now();
    const enable = () => {
      for (const b of actionButtons) b.disabled = false;
    };
    if (remaining > 0) unlockTimer = setTimeout(enable, remaining);
    else enable();
  }

  async function loadImage(img: HTMLImageElement, url: string | null): Promise<boolean> {
    if (!url) return false;
    try {
      img.src = await api.fetchImage(url);
      return true;
    } catch (e) {
      // 401 ならバックエンドはセッションを破棄済みなのでログイン画面へ戻す。それ以外は画像無しで続行
      if ((e as ApiError)?.kind === "unauthorized") handleError(e);
      return false;
    }
  }

  // ---------------------------------------------------------------- ヘッダー

  const refreshButton = el("button", { class: "icon-btn white", type: "button", title: "フレンド・ワールド情報を更新" }, icon("refresh", 22));
  const logoutButton = el("button", { class: "btn text", type: "button" }, icon("logout", 18), el("span", { text: "ログアウト" })) as HTMLButtonElement;
  const sep = () => el("span", { class: "sep", "aria-hidden": "true" });
  const headerRight = el("div", { class: "banner-right" }, el("span", { class: "user-name", text: opts.user.displayName }), sep(), refreshButton, sep(), logoutButton);

  logoutButton.addEventListener("click", async () => {
    logoutButton.disabled = true;
    try {
      await api.logout();
    } catch (e) {
      // 401 は handleError が onSessionExpired でログイン画面へ戻すので二重に遷移しない
      if (handleError(e, "ログアウトエラー")) return;
    }
    opts.onLogout();
  });

  // ---------------------------------------------------------------- インスタンス作成

  const worldSearch = el("input", { type: "text", class: "with-icon", placeholder: "お気に入り・自作ワールドから検索" }) as HTMLInputElement;
  const worldDropdown = el("div", { class: "dropdown hidden", role: "listbox" });
  const worldId = el("input", { type: "text", placeholder: "wrld_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx", spellcheck: "false" }) as HTMLInputElement;
  const typeSelect = el("select") as HTMLSelectElement;
  for (const t of INSTANCE_TYPES) typeSelect.append(el("option", { value: t.value, text: t.label }));
  const regionSelect = el("select") as HTMLSelectElement;
  for (const r of REGIONS) regionSelect.append(el("option", { value: r, text: r }));
  const createButton = el("button", { class: "btn", type: "button" }, icon("addCircle", 18), el("span", { text: "インスタンスを作成" })) as HTMLButtonElement;
  const locationField = el("input", { type: "text", readonly: true }) as HTMLInputElement;
  const copyButton = el("button", { class: "icon-btn", type: "button", title: "コピー" }, icon("copy", 20));
  const locationRow = el("div", { class: "row hidden" }, el("div", { class: "field" }, el("label", { text: "インスタンス場所" }), locationField), copyButton);

  const worldLoading = spinner(20);
  const worldThumb = el("img", { class: "thumb", alt: "" }) as HTMLImageElement;
  const worldName = el("div", { class: "name" });
  const worldError = el("div", { class: "msg error" });
  const worldPanel = el("div", { class: "side-panel world" }, worldLoading, worldThumb, worldName, worldError);
  show(worldLoading, false);
  show(worldThumb, false);
  show(worldName, false);
  show(worldError, false);

  let worldFetchSeq = 0;
  async function fetchWorldInfo(id: string): Promise<void> {
    const seq = ++worldFetchSeq;
    id = id.trim();
    // Python 版と同じく wrld_ で始まらない入力途中の値では何も表示しない
    if (!id.startsWith("wrld_")) {
      show(worldThumb, false);
      show(worldName, false);
      show(worldError, false);
      show(worldLoading, false);
      return;
    }
    show(worldLoading, true);
    show(worldThumb, false);
    show(worldName, false);
    show(worldError, false);
    try {
      const world = await api.getWorld(id);
      if (seq !== worldFetchSeq) return;
      const ok = await loadImage(worldThumb, worldThumbnailUrl(world));
      if (seq !== worldFetchSeq) return;
      show(worldThumb, ok);
      worldName.textContent = world.name;
      show(worldName, true);
    } catch (e) {
      if (seq !== worldFetchSeq) return;
      const err = e as ApiError;
      if (err.kind === "unauthorized") {
        handleError(err);
        return;
      }
      worldError.textContent = err.status === 404 ? "ワールドが見つかりません" : err.kind === "validation" ? "ワールドIDの形式が正しくありません" : `エラー: ${err.status ?? err.message}`;
      show(worldError, true);
    } finally {
      if (seq === worldFetchSeq) show(worldLoading, false);
    }
  }

  worldId.addEventListener("input", debounce(() => void fetchWorldInfo(worldId.value), 1000));

  /** 「お気に入り」「自作ワールド」の 2 グループに分けて候補を描画する。 */
  function renderWorldDropdown(list: WorldEntry[]): void {
    const item = (w: WorldEntry): HTMLElement => {
      const tags: HTMLElement[] = [];
      if (w.favorite && w.own) tags.push(el("span", { class: "item-tag", text: "自作" }));
      if (w.own && w.releaseStatus && w.releaseStatus !== "public") tags.push(el("span", { class: "item-tag", text: "非公開" }));
      return el(
        "button",
        {
          class: "dropdown-item",
          type: "button",
          onMousedown: (ev) => ev.preventDefault(), // blur より先に選択させる
          onClick: () => selectWorld(w),
        },
        el("span", { class: "item-name" }, el("span", { text: w.name }), ...tags),
        el("span", { class: "item-id", text: w.id }),
      );
    };
    const nodes: HTMLElement[] = [];
    const group = (label: string, items: WorldEntry[]): void => {
      if (items.length === 0) return;
      nodes.push(el("div", { class: "dropdown-group", role: "presentation", text: label }));
      for (const w of items.slice(0, DROPDOWN_LIMIT)) nodes.push(item(w));
    };
    group("お気に入り", list.filter((w) => w.favorite));
    group("自作ワールド", list.filter((w) => w.own && !w.favorite));
    worldDropdown.replaceChildren(...nodes);
    show(worldDropdown, nodes.length > 0);
  }

  function selectWorld(w: WorldEntry): void {
    worldSearch.value = w.name;
    worldId.value = w.id;
    show(worldDropdown, false);
    void fetchWorldInfo(w.id);
  }

  worldSearch.addEventListener("input", () => {
    const q = worldSearch.value.trim().toLowerCase();
    if (!q) {
      show(worldDropdown, false);
      return;
    }
    renderWorldDropdown(worlds.filter((w) => w.name.toLowerCase().includes(q) || w.id.toLowerCase().includes(q)));
  });
  worldSearch.addEventListener("focus", () => {
    if (worlds.length > 0 && !worldSearch.value) renderWorldDropdown(worlds);
  });
  worldSearch.addEventListener("blur", () => setTimeout(() => show(worldDropdown, false), 200));

  createButton.addEventListener("click", async () => {
    setButtonsDisabled(true);
    try {
      const instance = await api.createInstance(worldId.value.trim(), typeSelect.value as UiInstanceType, regionSelect.value as Region);
      locationField.value = instance.location;
      show(locationRow, true);
      inviteInstanceId.value = instance.location;
    } catch (e) {
      handleError(e);
    } finally {
      setButtonsDisabled(false);
    }
  });

  copyButton.addEventListener("click", async () => {
    const text = locationField.value;
    if (!text) return;
    try {
      await writeText(text);
    } catch {
      try {
        await navigator.clipboard.writeText(text);
      } catch (e) {
        logStore.append(`クリップボードへのコピーに失敗しました: ${String(e)}`);
        return;
      }
    }
    logStore.append("インスタンス場所をコピーしました。");
  });

  const instanceCard = card(
    sectionHeader("dns", "インスタンスを作成"),
    el(
      "div",
      { class: "split" },
      el(
        "div",
        { class: "col" },
        el("div", { class: "field" }, el("label", { text: "ワールド検索 (名前 or ID)" }), el("span", { class: "prefix-icon" }, icon("search", 18)), worldSearch),
        worldDropdown,
        el("div", { class: "field" }, el("label", { text: "ワールドID" }), worldId),
        el(
          "div",
          { class: "row" },
          el("div", { class: "field" }, el("label", { text: "インスタンスタイプ" }), typeSelect),
          el("div", { class: "field" }, el("label", { text: "リージョン" }), regionSelect),
        ),
        el("div", { class: "row", style: "margin-top:6px" }, createButton),
        locationRow,
      ),
      worldPanel,
    ),
  );

  // ---------------------------------------------------------------- 招待

  const friendSearch = el("input", { type: "text", class: "with-icon", placeholder: "フレンド名、または usr_... を直接入力" }) as HTMLInputElement;
  const friendDropdown = el("div", { class: "dropdown hidden", role: "listbox" });
  /** フォーカスを外したときの名前解決の結果 (候補が複数 / 無い) を短く表示する。 */
  const friendHint = el("p", { class: "msg warning field-hint hidden", "aria-live": "polite" });
  const chips = el("div", { class: "chips" });
  const counter = el("div", { class: "counter hidden" });
  const inviteInstanceId = el("input", { type: "text", placeholder: "wrld_...:12345~region(jp)", spellcheck: "false" }) as HTMLInputElement;
  const inviteButton = el("button", { class: "btn invite", type: "button" }, icon("personAdd", 18), el("span", { text: "招待する" })) as HTMLButtonElement;
  const selfInviteButton = el("button", { class: "btn self-invite", type: "button" }, icon("person", 18), el("span", { text: "自分に招待を送る" })) as HTMLButtonElement;

  const userLoading = spinner(20);
  const userIcon = el("img", { class: "avatar", alt: "" }) as HTMLImageElement;
  const userName = el("div", { class: "name" });
  const userError = el("div", { class: "msg error" });
  const userPanel = el("div", { class: "side-panel user" }, userLoading, userIcon, userName, userError);
  show(userLoading, false);
  show(userIcon, false);
  show(userName, false);
  show(userError, false);

  /** 選択中フレンド (挿入順を保つ)。 */
  const selected = new Map<string, string>();

  function resetUserPanel(): void {
    show(userLoading, false);
    show(userIcon, false);
    show(userName, false);
    show(userError, false);
  }

  let userFetchSeq = 0;
  async function fetchUserInfo(id: string): Promise<void> {
    const seq = ++userFetchSeq;
    resetUserPanel();
    if (!id) return;
    show(userLoading, true);
    try {
      const user = await api.getUser(id);
      if (seq !== userFetchSeq) return;
      const ok = await loadImage(userIcon, userIconUrl(user));
      if (seq !== userFetchSeq) return;
      show(userIcon, ok);
      userName.textContent = user.displayName;
      show(userName, true);
    } catch (e) {
      if (seq !== userFetchSeq) return;
      const err = e as ApiError;
      if (err.kind === "unauthorized") {
        handleError(err);
        return;
      }
      userError.textContent = err.status === 404 ? "ユーザーが見つかりません" : err.kind === "validation" ? "ユーザーIDの形式が正しくありません" : `エラー: ${err.status ?? err.message}`;
      show(userError, true);
    } finally {
      if (seq === userFetchSeq) show(userLoading, false);
    }
  }

  function updateCounter(): void {
    counter.textContent = `選択中: ${selected.size} / ${MAX_SELECTED_FRIENDS}`;
    show(counter, selected.size > 0);
  }

  function rebuildChips(): void {
    chips.replaceChildren(
      ...[...selected.entries()].map(([id, name]) =>
        el(
          "span",
          { class: "chip" },
          el("span", { text: name }),
          el("button", { type: "button", title: "削除", "aria-label": `${name} を削除`, onClick: () => removeFriend(id) }, icon("close", 16)),
        ),
      ),
    );
    updateCounter();
  }

  function firstSelectedId(): string | undefined {
    return selected.keys().next().value;
  }

  function removeFriend(id: string): void {
    const wasFirst = firstSelectedId() === id;
    selected.delete(id);
    rebuildChips();
    if (selected.size === 0) {
      userFetchSeq++;
      resetUserPanel();
    } else if (wasFirst) {
      void fetchUserInfo(firstSelectedId() ?? "");
    }
  }

  function setFriendHint(text: string): void {
    friendHint.textContent = text;
    show(friendHint, text.length > 0);
  }

  function selectFriend(f: Friend): void {
    show(friendDropdown, false);
    friendSearch.value = "";
    setFriendHint("");
    if (selected.has(f.id)) return;
    if (selected.size >= MAX_SELECTED_FRIENDS) {
      logStore.append(`選択上限(${MAX_SELECTED_FRIENDS}人)に達しています。`);
      return;
    }
    const isFirst = selected.size === 0;
    selected.set(f.id, f.displayName);
    rebuildChips();
    if (isFirst) void fetchUserInfo(f.id);
  }

  /** 候補クリック中は blur 側の名前解決を抑止する (クリックによる選択を優先させる)。 */
  let pickingFriend = false;

  function renderFriendDropdown(list: Friend[]): void {
    friendDropdown.replaceChildren(
      ...list.slice(0, DROPDOWN_LIMIT).map((f) =>
        el(
          "button",
          {
            class: "dropdown-item",
            type: "button",
            onMousedown: (ev) => {
              ev.preventDefault(); // 入力欄の blur を起こさず、クリックで選択させる
              pickingFriend = true;
              setTimeout(() => (pickingFriend = false), 300);
            },
            onClick: () => {
              pickingFriend = false;
              selectFriend(f);
            },
          },
          el("span", { class: "item-name", text: f.displayName }),
          el("span", { class: "item-id", text: f.id }),
        ),
      ),
    );
    show(friendDropdown, list.length > 0);
  }

  /**
   * 入力欄の文字列からフレンドを決める (ワールドID欄と同様、フォーカスを外したとき / Enter で動く)。
   * `usr_...` はそのまま選択して表示名を後から取得する。名前は 完全一致 → 前方一致 → 部分一致 の順で
   * ちょうど 1 人に絞れたときだけ選択し、複数 / 0 件ならヒントを出して何もしない。
   */
  function resolveFriendInput(): void {
    const text = friendSearch.value.trim();
    if (!text) return;
    if (/^usr_/i.test(text)) {
      if (!USER_ID_RE.test(text)) {
        setFriendHint("ユーザーIDの形式が正しくありません (usr_xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx)");
        return;
      }
      const id = text.toLowerCase().startsWith("usr_") ? `usr_${text.slice(4).toLowerCase()}` : text;
      const known = friends.find((f) => f.id === id);
      const before = selected.size;
      selectFriend(known ?? { id, displayName: id });
      if (!known && selected.size > before) {
        // 表示名を取得してチップの表示を差し替える (取得できなくても ID のまま残す)
        void api.getUser(id).then(
          (u) => {
            if (selected.has(id)) {
              selected.set(id, u.displayName);
              rebuildChips();
            }
          },
          (e: ApiError) => {
            if (e.kind === "unauthorized") handleError(e);
          },
        );
      }
      return;
    }
    const q = text.toLowerCase();
    const pick = (pred: (name: string) => boolean): Friend[] | null => {
      const hits = friends.filter((f) => pred(f.displayName.toLowerCase()));
      return hits.length === 0 ? null : hits;
    };
    const hits = pick((n) => n === q) ?? pick((n) => n.startsWith(q)) ?? pick((n) => n.includes(q));
    if (!hits) {
      setFriendHint("該当するフレンドがいません。");
      return;
    }
    if (hits.length > 1) {
      setFriendHint(`候補が ${hits.length} 人います。候補から選択してください。`);
      return;
    }
    selectFriend(hits[0]!);
  }

  friendSearch.addEventListener("input", () => {
    setFriendHint("");
    const q = friendSearch.value.trim().toLowerCase();
    if (!q) {
      show(friendDropdown, false);
      return;
    }
    renderFriendDropdown(friends.filter((f) => f.displayName.toLowerCase().includes(q) || f.id.toLowerCase().includes(q)));
  });
  friendSearch.addEventListener("focus", () => {
    if (friends.length > 0 && !friendSearch.value) renderFriendDropdown(friends);
  });
  friendSearch.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") {
      ev.preventDefault();
      resolveFriendInput();
    } else if (ev.key === "Escape") {
      show(friendDropdown, false);
    }
  });
  friendSearch.addEventListener("blur", () => {
    setTimeout(() => show(friendDropdown, false), 200);
    if (!pickingFriend) resolveFriendInput();
  });

  inviteButton.addEventListener("click", async () => {
    setButtonsDisabled(true);
    try {
      const location = inviteInstanceId.value.trim();
      if (!location) {
        logStore.append("エラー: インスタンスIDを入力してください。");
        return;
      }
      let targets = [...selected.entries()].map(([id, name]) => ({ id, name }));
      if (targets.length === 0) {
        const direct = friendSearch.value.trim();
        if (!direct) {
          logStore.append("エラー: ユーザーを選択または入力してください。");
          return;
        }
        targets = [{ id: direct, name: direct }];
      }
      const report = await api.inviteUsers(location, targets);
      // 成功した相手だけ選択から外す (失敗分は再送できるよう残す)
      for (const r of report.results) {
        if (r.ok) selected.delete(r.userId);
      }
      rebuildChips();
      userFetchSeq++;
      resetUserPanel();
      friendSearch.value = "";
      if (selected.size > 0) void fetchUserInfo(firstSelectedId() ?? "");
    } catch (e) {
      handleError(e);
    } finally {
      setButtonsDisabled(false);
    }
  });

  selfInviteButton.addEventListener("click", async () => {
    setButtonsDisabled(true);
    try {
      const location = inviteInstanceId.value.trim();
      if (!location) {
        logStore.append("エラー: インスタンスIDを入力してください。");
        return;
      }
      await api.inviteSelf(location);
    } catch (e) {
      handleError(e);
    } finally {
      setButtonsDisabled(false);
    }
  });

  const inviteCard = card(
    sectionHeader("mail", "招待"),
    el(
      "div",
      { class: "split" },
      el(
        "div",
        { class: "col" },
        el("div", { class: "field" }, el("label", { text: "フレンド検索 (名前 or ID)" }), el("span", { class: "prefix-icon" }, icon("search", 18)), friendSearch),
        friendDropdown,
        friendHint,
        chips,
        counter,
        el("div", { class: "field" }, el("label", { text: "インスタンスID" }), inviteInstanceId),
        el("div", { class: "row wrap" }, inviteButton, selfInviteButton),
      ),
      userPanel,
    ),
  );

  actionButtons.push(createButton, inviteButton, selfInviteButton);

  // ---------------------------------------------------------------- ログ

  const logPanel = el("pre", { class: "log", "aria-live": "polite" });
  const clearLogButton = el("button", { class: "btn text danger", type: "button" }, icon("delete", 18), el("span", { text: "クリア" }));
  clearLogButton.addEventListener("click", () => logStore.clear());
  const unsubscribeLog = logStore.subscribe((lines) => {
    logPanel.textContent = lines.join("\n");
    logPanel.scrollTop = logPanel.scrollHeight;
  });
  const logCard = card(sectionHeader("terminal", "ログ", clearLogButton), logPanel);

  // ---------------------------------------------------------------- データ取得

  const loadingSpinner = spinner(40);
  loadingSpinner.classList.add("large");
  const loadingDialog = el("dialog", { class: "loading" }, loadingSpinner, el("p", { text: "フレンド一覧・ワールドを取得中..." })) as HTMLDialogElement;
  loadingDialog.addEventListener("cancel", (ev) => ev.preventDefault());

  async function refreshData(): Promise<void> {
    if (!loadingDialog.isConnected) document.body.append(loadingDialog);
    if (!loadingDialog.open) loadingDialog.showModal();
    try {
      friends = await api.listFriends();
      worlds = await api.listWorlds();
      const favCount = worlds.filter((w) => w.favorite).length;
      const ownCount = worlds.filter((w) => w.own).length;
      logStore.append(`フレンド ${friends.length} 人、お気に入りワールド ${favCount} 件、自作ワールド ${ownCount} 件を取得しました。`);
    } catch (e) {
      handleError(e, "データ取得エラー");
    } finally {
      if (loadingDialog.open) loadingDialog.close();
    }
  }
  refreshButton.addEventListener("click", () => void refreshData());

  const root = el("div", { class: "col" }, banner(headerRight), instanceCard, inviteCard, logCard);

  // 画面を離れるときにダイアログとログ購読を片付ける (ログイン/ログアウトを繰り返してもリークしない)
  const observer = new MutationObserver(() => {
    if (!root.isConnected) {
      loadingDialog.remove();
      unsubscribeLog();
      observer.disconnect();
    }
  });
  observer.observe(document.getElementById("app") ?? document.body, { childList: true });

  return { root, refreshData };
}
