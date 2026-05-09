import os
import sys
import tkinter as tk
from tkinter import ttk, messagebox
from pathlib import Path

# --- MAC OS PATH ADAPTATION ---
# Ensure root directory is in sys.path for imports
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '../..')))

import src.config
from src.config import load_config, save_config, T as _T, CONFIG_DIR
from src.app_state import get_auto_start_state, set_auto_start

# --- macOS Local Translations ---
_MAC_T = {
    "zh": {"currency": "货币"},
    "en": {"currency": "Currency"}
}

def T(key, lang="zh", **kwargs):
    """Local translation wrapper that falls back to global T."""
    text = _MAC_T.get(lang, _MAC_T["zh"]).get(key)
    if text:
        return text.format(**kwargs) if kwargs else text
    return _T(key, lang, **kwargs)
from src.mac.keystore import encrypt_api_key, decrypt_api_key

# ─── Eye icon (SVG-style drawn on Canvas) ─────────────────────────────────────
_EYE_OPEN = (
    "M8 5C4.5 5 1.5 8 1.5 8S4.5 11 8 11 14.5 8 14.5 8 11.5 5 8 5z "
    "M8 10a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"
)
_EYE_CLOSED = (
    "M2 2 L14 14 M8 5C4.5 5 1.5 8 1.5 8S4.5 11 8 11 14.5 8 14.5 8"
)

def _make_eye_button(parent, entry_widget, show_var: tk.BooleanVar):
    """Draw an eye icon on a Canvas that toggles password visibility."""
    BTN = 28
    c = tk.Canvas(parent, width=BTN, height=BTN, highlightthickness=0,
                  cursor="hand2")
    # ttk widgets don't support "bg"; use systemWindowBackgroundColor for native look
    try:
        c.configure(bg="systemWindowBackgroundColor")
    except Exception:
        c.configure(bg="white")

    def _redraw():
        c.delete("all")
        is_visible = show_var.get()
        color = "gray" # Subtle gray
        
        cx, cy = 14, 14
        # Smaller eye base (oval)
        c.create_oval(cx-7, cy-4, cx+7, cy+4, outline=color, width=1.5)
        
        if is_visible:
            # Eye opened: Pupil
            c.create_oval(cx-2, cy-2, cx+2, cy+2, fill=color, outline=color)
        else:
            # Eye closed: Smaller pupil + Diagonal slash
            c.create_oval(cx-1, cy-1, cx+1, cy+1, fill=color, outline=color)
            c.create_line(cx-9, cy-6, cx+9, cy+6, fill=color, width=1.5, capstyle="round")

    def _toggle(_event=None):
        show_var.set(not show_var.get())
        entry_widget.config(show="" if show_var.get() else "•")
        _redraw()

    c.bind("<Button-1>", _toggle)
    _redraw()
    return c


class Tooltip:
    """A simple tooltip implementation for Tkinter widgets."""
    def __init__(self, widget, text, delay=1000):
        self.widget = widget
        self.text = text
        self.delay = delay
        self.tip_window = None
        self.id = None
        self.widget.bind("<Enter>", self.enter)
        self.widget.bind("<Leave>", self.leave)

    def enter(self, event=None):
        self.schedule()

    def leave(self, event=None):
        self.unschedule()
        self.hide_tip()

    def schedule(self):
        self.unschedule()
        self.id = self.widget.after(self.delay, self.show_tip)

    def unschedule(self):
        id_ = self.id
        self.id = None
        if id_:
            self.widget.after_cancel(id_)

    def show_tip(self):
        if self.tip_window or not self.text:
            return
        x, y, _cx, cy = self.widget.bbox("insert")
        x = x + self.widget.winfo_rootx() + 25
        y = y + cy + self.widget.winfo_rooty() + 25
        self.tip_window = tw = tk.Toplevel(self.widget)
        tw.wm_overrideredirect(True)
        tw.wm_geometry(f"+{x}+{y}")
        label = tk.Label(tw, text=self.text, justify='left',
                         background="#ffffe0", relief='solid', borderwidth=1,
                         font=("system", 12, "normal"))
        label.pack(ipadx=1)

    def hide_tip(self):
        tw = self.tip_window
        self.tip_window = None
        if tw:
            tw.destroy()


def run_settings():
    config = load_config()
    lang = config.get("language", "zh")
    CTRL_W = 18  # uniform width for all controls (in text units)

    root = tk.Tk()
    root.title(T("settings_title", lang))
    root.resizable(False, False)

    # Force geometry update to get accurate winfo_width/height
    root.update_idletasks()
    # For macOS, winfo_reqwidth/height are often more accurate for fixed-size windows before they are drawn
    w = root.winfo_reqwidth()
    h = root.winfo_reqheight()
    # Fallback to defaults if req is too small
    w = max(w, 420)
    h = max(h, 480)
    
    sw = root.winfo_screenwidth()
    sh = root.winfo_screenheight()
    x = (sw - w) // 2
    y = (sh - h) // 2
    root.geometry(f"{w}x{h}+{x}+{y}")

    root.lift()
    root.attributes('-topmost', True)
    root.after_idle(root.attributes, '-topmost', False)
    root.focus_force()

    style = ttk.Style()
    # Let Tkinter use its default platform theme

    # Use system colors
    bg_color = "systemWindowBackgroundColor"
    fg_color = "systemTextColor"
    
    root.configure(bg=bg_color)
    
    style.configure("TFrame", background=bg_color)
    style.configure("TLabel", font=("system", 13), background=bg_color, foreground=fg_color)
    style.configure("TCheckbutton", font=("system", 13), background=bg_color, foreground=fg_color)
    style.configure("Title.TLabel", font=("system", 26, "bold"), background=bg_color, foreground=fg_color)
    
    # Standardize Button font
    style.configure("TButton", font=("system", 13))

    # ── Header ────────────────────────────────────────────────────────────────
    header = ttk.Frame(root)
    header.pack(fill="x", pady=(30, 20), padx=30)
    banner_sub = "配置您的账号与预警偏好。" if lang == "zh" else "Configure your account and monitor preferences."
    ttk.Label(header, text="DeepSeek Balance", style="Title.TLabel").pack(anchor="w")
    ttk.Label(header, text=banner_sub, foreground="gray", font=("system", 12)).pack(anchor="w")

    # ── Content ───────────────────────────────────────────────────────────────
    content = ttk.Frame(root)
    content.pack(fill="both", expand=True, padx=30, pady=(0, 10))

    # ── Grid of controls ──────────────────────────────────────────────────────
    grid_frame = ttk.Frame(content)
    grid_frame.pack(fill="x", pady=4)
    # Column 0: labels, Column 1: Entry/Combobox/Spinbox, Column 2: Eye button
    grid_frame.columnconfigure(0, weight=0)
    grid_frame.columnconfigure(1, weight=1) # Allow input column to expand
    grid_frame.columnconfigure(2, weight=0)

    def _label(row, text):
        ttk.Label(grid_frame, text=text).grid(row=row, column=0, sticky="w", pady=8, padx=(0,12))

    def _spinbox(row, var, **kw):
        sb = ttk.Spinbox(grid_frame, textvariable=var, font=("system", 13), width=CTRL_W, **kw)
        # sticky="ew" ensures it fills column 1 and 2
        sb.grid(row=row, column=1, sticky="ew", columnspan=2)
        return sb

    def _combo(row, var, values):
        cb = ttk.Combobox(grid_frame, textvariable=var, values=values,
                          state="readonly", font=("system", 13), width=CTRL_W)
        # sticky="ew" ensures it fills column 1 and 2
        cb.grid(row=row, column=1, sticky="ew", columnspan=2)
        return cb

    # --- API KEY (Row 0) ---
    _label(0, T("api_key_label", lang))
    decrypted_key = decrypt_api_key(config.get("api_key_enc", ""), CONFIG_DIR)
    if not decrypted_key:
        decrypted_key = config.get("api_key", "")
    api_var = tk.StringVar(value=decrypted_key)
    show_var = tk.BooleanVar(value=False)
    # Use consistent font size 13
    api_entry = ttk.Entry(grid_frame, textvariable=api_var, show="•", width=CTRL_W-4, font=("system", 13))
    api_entry.grid(row=0, column=1, sticky="ew")
    # Make canvas background match the container
    eye_btn = _make_eye_button(grid_frame, api_entry, show_var)
    eye_btn.configure(bg=bg_color)
    eye_btn.grid(row=0, column=2, padx=(8, 0), sticky="w")

    interval_label = T("interval_label", lang)
    _label(1, interval_label)
    interval_var = tk.IntVar(value=config.get("interval_minutes", 10))
    sb_interval = _spinbox(1, interval_var, from_=1, to=1440)
    Tooltip(sb_interval, T("interval_hint", lang).strip())

    _label(2, T("threshold_label", lang))
    threshold_var = tk.DoubleVar(value=config.get("threshold_yuan", 1.0))
    sb_threshold = _spinbox(2, threshold_var, from_=0.0, to=10000.0, increment=0.5)
    Tooltip(sb_threshold, T("threshold_hint", lang).strip())

    _label(3, T("language_label", lang))
    LANG_OPTIONS = {"中文": "zh", "English": "en"}
    cur_lang = {v: k for k, v in LANG_OPTIONS.items()}.get(config.get("language", "zh"), "中文")
    lang_var = tk.StringVar(value=cur_lang)
    _combo(3, lang_var, list(LANG_OPTIONS.keys()))

    _label(4, T("currency", lang) + " / Currency:")
    CUR_OPTIONS = ["CNY", "USD"]
    cur_var = tk.StringVar(value=config.get("currency", "CNY"))
    _combo(4, cur_var, CUR_OPTIONS)

    enable_alerts_var = tk.BooleanVar(value=config.get("enable_alerts", True))
    ttk.Checkbutton(content, text=T("enable_alerts_label", lang),
                    variable=enable_alerts_var).pack(anchor="w", pady=(12, 0))

    auto_start_var = tk.BooleanVar(value=config.get("auto_start", False) or get_auto_start_state())
    ttk.Checkbutton(content, text=T("auto_start_label", lang),
                    variable=auto_start_var).pack(anchor="w", pady=(8, 4))

    # ── Credits ───────────────────────────────────────────────────────────────
    footer_info = ttk.Frame(content)
    footer_info.pack(fill="x", pady=(15, 0))
    ttk.Label(footer_info, text="V1.0.1_260508", foreground="gray", font=("system", 11)).pack(anchor="w")
    ttk.Label(footer_info, text="GitHub @SrtaEstrella  |  RedNote @Estella_han",
              foreground="gray", font=("system", 11)).pack(anchor="w")

    # ── Buttons ───────────────────────────────────────────────────────────────
    # Add a flexible spacer to push buttons down if needed, or just larger pady
    btn_frame = ttk.Frame(root)
    btn_frame.pack(fill="x", pady=(30, 20), padx=30)

    def on_save():
        key = api_var.get().strip()
        if not key:
            messagebox.showwarning(T("warn_title", lang), T("warn_no_key", lang), parent=root)
            return
        config["api_key_enc"] = encrypt_api_key(key, CONFIG_DIR)
        config.pop("api_key", None)           # remove any legacy plain-text
        config["interval_minutes"] = interval_var.get()
        config["threshold_yuan"] = threshold_var.get()
        config["language"] = LANG_OPTIONS.get(lang_var.get(), "zh")
        config["currency"] = cur_var.get()
        config["enable_alerts"] = enable_alerts_var.get()
        config["auto_start"] = auto_start_var.get()
        set_auto_start(config["auto_start"])
        save_config(config)
        root.destroy()

    def _cleanup():
        root.destroy()

    # Center buttons using an inner frame with expand=True
    btn_container = ttk.Frame(btn_frame)
    btn_container.pack(expand=True)

    ttk.Button(btn_container, text=T("cancel", lang), command=_cleanup).pack(side="left", padx=10)
    save_btn = ttk.Button(btn_container, text=T("save", lang), command=on_save, default="active")
    save_btn.pack(side="left", padx=10)

    root.bind("<Return>", lambda e: save_btn.invoke())
    root.bind("<Escape>", lambda e: _cleanup())
    api_entry.focus_set()
    root.mainloop()


if __name__ == "__main__":
    run_settings()
