// ログイン画面 + 二段階認証ダイアログ。
import { api, type ApiError, type LoginOutcome, type UserSummary } from "../api";
import { banner, card, el, icon, sectionHeader, show, spinner } from "../ui";

/** 2FA ダイアログの応答待ちタイムアウト (Python 版と同じ 5 分)。 */
const TWO_FACTOR_TIMEOUT_MS = 300_000;

export interface LoginViewOptions {
  savedUsername: string;
  /** 初期表示のメッセージ (セッション切れ等)。 */
  message?: { text: string; level: "error" | "warning" | "info" };
  onLoggedIn: (user: UserSummary) => void;
}

export interface LoginView {
  root: HTMLElement;
  /** 起動時自動ログインの結果を流し込む。 */
  handleOutcome(outcome: LoginOutcome): Promise<void>;
  setChecking(checking: boolean): void;
  showError(e: ApiError | string): void;
}

export function createLoginView(opts: LoginViewOptions): LoginView {
  const username = el("input", { type: "text", autocomplete: "username", value: opts.savedUsername, autofocus: true }) as HTMLInputElement;
  const password = el("input", { type: "password", autocomplete: "current-password" }) as HTMLInputElement;
  const reveal = el("button", { class: "icon-btn suffix-btn", type: "button", title: "パスワードを表示" }, icon("visibility", 18));
  reveal.addEventListener("click", () => {
    password.type = password.type === "password" ? "text" : "password";
  });

  const message = el("p", { class: "msg hidden" });
  const loading = spinner(20);
  show(loading, false);
  const loginButton = el("button", { class: "btn", type: "submit" }, icon("login", 18), el("span", { text: "ログイン" })) as HTMLButtonElement;

  function setMessage(text: string, level: "error" | "warning" | "info" = "error"): void {
    message.textContent = text;
    message.className = `msg ${level}`;
    show(message, text.length > 0);
  }

  function setBusy(busy: boolean): void {
    loginButton.disabled = busy;
    show(loading, busy);
  }

  const form = el(
    "form",
    { class: "col", autocomplete: "on" },
    el("div", { class: "field" }, el("label", { text: "ユーザー名" }), username),
    el("div", { class: "field" }, el("label", { text: "パスワード" }), password, reveal),
    el("div", { class: "row" }, loginButton, loading),
    message,
  ) as HTMLFormElement;

  form.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const uname = username.value.trim();
    const pwd = password.value;
    if (!uname || !pwd) {
      setMessage("ユーザー名とパスワードを入力してください。");
      return;
    }
    setBusy(true);
    setMessage("");
    try {
      const outcome = await api.login(uname, pwd);
      password.value = "";
      await handleOutcome(outcome);
    } catch (e) {
      showError(e as ApiError);
    }
  });

  const root = el("div", { class: "col" }, banner(), card(sectionHeader("lock", "ログイン"), form));

  function showError(e: ApiError | string): void {
    setBusy(false);
    if (typeof e === "string") {
      setMessage(e);
      return;
    }
    if (e.kind === "login_failed" || e.kind === "validation" || e.kind === "two_factor_failed" || e.kind === "login") {
      setMessage(e.message);
    } else {
      setMessage(`エラー: ${e.message}`);
    }
  }

  async function handleOutcome(outcome: LoginOutcome): Promise<void> {
    switch (outcome.status) {
      case "logged_in":
        setBusy(false);
        setMessage("");
        opts.onLoggedIn(outcome.user);
        return;
      case "two_factor_required": {
        setBusy(true);
        const user = await runTwoFactorDialog(outcome.email);
        if (user) {
          setBusy(false);
          opts.onLoggedIn(user);
        } else {
          setBusy(false);
        }
        return;
      }
      case "not_logged_in":
        setBusy(false);
        if (outcome.message) setMessage(outcome.message, "warning");
        else setMessage("");
        return;
    }
  }

  /** 2FA ダイアログを表示し、ログイン完了ならユーザーを返す。キャンセル/失敗は null。 */
  function runTwoFactorDialog(email: boolean): Promise<UserSummary | null> {
    return new Promise((resolve) => {
      const prompt = email ? "メールに届いた2FAコードを入力してください:" : "認証アプリの2FAコードを入力してください:";
      const codeInput = el("input", { type: "text", inputmode: "text", autocomplete: "one-time-code", placeholder: "123456 または xxxx-xxxx" }) as HTMLInputElement;
      const err = el("p", { class: "msg error hidden" });
      const okButton = el("button", { class: "btn", type: "submit" }, el("span", { text: "OK" })) as HTMLButtonElement;
      const cancelButton = el("button", { class: "btn text", type: "button" }, el("span", { text: "キャンセル" })) as HTMLButtonElement;
      const busy = spinner(18);
      show(busy, false);

      const dialogForm = el(
        "form",
        { method: "dialog" },
        el("p", { text: prompt }),
        el("div", { class: "field" }, el("label", { text: "2FAコード" }), codeInput),
        err,
        el("div", { class: "actions" }, busy, cancelButton, okButton),
      ) as HTMLFormElement;
      const dialog = el("dialog", { "aria-labelledby": "tfa-title" }, el("h2", { id: "tfa-title", text: "二段階認証" }), dialogForm) as HTMLDialogElement;
      document.body.append(dialog);

      let finished = false;
      const timer = setTimeout(() => finish(null, "2FAコードの入力がタイムアウトしました。ログインをやり直してください。"), TWO_FACTOR_TIMEOUT_MS);

      async function finish(user: UserSummary | null, errorText?: string): Promise<void> {
        if (finished) return;
        finished = true;
        clearTimeout(timer);
        dialog.close();
        dialog.remove();
        if (!user) {
          try {
            await api.cancelLogin();
          } catch {
            /* 無視 */
          }
          if (errorText) setMessage(errorText, "warning");
          else setMessage("ログインをキャンセルしました。", "warning");
        }
        resolve(user);
      }

      cancelButton.addEventListener("click", () => void finish(null));
      dialog.addEventListener("cancel", (ev) => {
        ev.preventDefault();
        void finish(null);
      });

      dialogForm.addEventListener("submit", async (ev) => {
        ev.preventDefault();
        const code = codeInput.value.trim();
        if (!code) {
          err.textContent = "2FAコードを入力してください。";
          show(err, true);
          return;
        }
        okButton.disabled = true;
        show(busy, true);
        try {
          const outcome = await api.submitTwoFactor(code);
          if (outcome.status === "logged_in") {
            void finish(outcome.user);
            return;
          }
          err.textContent = "ログインを完了できませんでした。";
          show(err, true);
        } catch (e) {
          const apiErr = e as ApiError;
          if (apiErr.kind === "two_factor_format" || apiErr.kind === "two_factor_invalid") {
            err.textContent = apiErr.message;
            show(err, true);
            codeInput.select();
          } else {
            // 試行回数超過・通信エラー等はダイアログを閉じてログイン画面に表示する
            finished = true;
            clearTimeout(timer);
            dialog.close();
            dialog.remove();
            showError(apiErr);
            resolve(null);
          }
        } finally {
          okButton.disabled = false;
          show(busy, false);
        }
      });

      dialog.showModal();
      codeInput.focus();
    });
  }

  function setChecking(checking: boolean): void {
    setBusy(checking);
    if (checking) setMessage("セッション確認中...", "info");
  }

  if (opts.message) setMessage(opts.message.text, opts.message.level);
  return { root, handleOutcome, setChecking, showError };
}
