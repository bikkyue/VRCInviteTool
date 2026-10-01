// ログパネルのバッファ。メイン画面が無いときも溜めておき、表示時にまとめて描画する。

const MAX_LINES = 2000;

type Listener = (lines: readonly string[]) => void;

class LogStore {
  private lines: string[] = [];
  private listeners = new Set<Listener>();

  append(message: string): void {
    for (const line of message.replace(/\n$/, "").split("\n")) {
      this.lines.push(line);
    }
    if (this.lines.length > MAX_LINES) {
      this.lines.splice(0, this.lines.length - MAX_LINES);
    }
    this.emit();
  }

  clear(): void {
    this.lines = [];
    this.emit();
  }

  snapshot(): readonly string[] {
    return this.lines;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    listener(this.lines);
    return () => this.listeners.delete(listener);
  }

  private emit(): void {
    for (const l of this.listeners) l(this.lines);
  }
}

export const logStore = new LogStore();
