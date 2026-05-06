"""
Settings dialog — tkinter window for configuring API key, interval, threshold,
preferred currency, and language.
"""
import threading


def open_settings(app):
    """Open the settings dialog (blocking)."""

    def _dialog():
        import os
        import sys
        import tkinter as tk
        from tkinter import ttk, messagebox

        from src.config import T, save_config, log, currency_sym

        lang = app.lang

        root = tk.Tk()
        # Set window icon — looks for app_icon.ico next to the exe (frozen) or script (dev)
        try:
            if getattr(sys, 'frozen', False):
                icon_path = os.path.join(sys._MEIPASS, 'app_icon.ico')
            else:
                icon_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                         'app_icon.ico')
            if os.path.isfile(icon_path):
                root.iconbitmap(icon_path)
        except Exception:
            pass

        root.title(T("settings_title", lang))
        root.geometry("490x710")
        root.resizable(True, True)
        root.minsize(460, 500)
        root.update_idletasks()
        sw, sh = root.winfo_screenwidth(), root.winfo_screenheight()
        w, h = root.winfo_width(), root.winfo_height()
        root.geometry(f"+{(sw - w) // 2}+{(sh - h) // 2}")

        frame = ttk.Frame(root, padding=(20, 20, 20, 10))
        frame.pack(fill="both", expand=True)

        # --- API Key ---
        ttk.Label(frame, text=T("api_key_label", lang)).pack(anchor="w")
        api_var = tk.StringVar(value=app.config.get("api_key", ""))
        api_entry = ttk.Entry(frame, textvariable=api_var, show="•", width=54)
        api_entry.pack(fill="x", pady=(0, 2))
        show_var = tk.BooleanVar(value=False)

        def toggle_show():
            api_entry.config(show="" if show_var.get() else "•")

        ttk.Checkbutton(frame, text=T("show_key", lang), variable=show_var,
                        command=toggle_show).pack(anchor="w", pady=(0, 8))

        ttk.Separator(frame, orient="horizontal").pack(fill="x", pady=5)

        # --- Interval ---
        ttk.Label(frame, text=T("interval_label", lang)).pack(anchor="w")
        interval_var = tk.IntVar(value=app.config.get("interval_minutes", 10))
        ifr = ttk.Frame(frame)
        ifr.pack(fill="x", pady=(0, 8))
        ttk.Spinbox(ifr, from_=1, to=1440, textvariable=interval_var, width=8).pack(side="left")
        ttk.Label(ifr, text=T("interval_hint", lang)).pack(side="left")

        # --- Threshold ---
        thresh_unit = "元" if lang == "zh" else "¥"
        ttk.Label(frame, text=T("threshold_label", lang, unit=thresh_unit)).pack(anchor="w")
        threshold_var = tk.DoubleVar(value=app.config.get("threshold_yuan", 1.0))
        tfr = ttk.Frame(frame)
        tfr.pack(fill="x", pady=(0, 8))
        ttk.Spinbox(tfr, from_=0.1, to=10000.0, increment=0.5,
                    textvariable=threshold_var, width=8).pack(side="left")
        ttk.Label(tfr, text=T("threshold_hint", lang)).pack(side="left")

        ttk.Separator(frame, orient="horizontal").pack(fill="x", pady=5)

        # --- Preferred Currency ---
        ttk.Label(frame, text=T("currency_label", lang)).pack(anchor="w")
        currency_var = tk.StringVar(value=app.config.get("preferred_currency", "CNY"))
        currency_list = ["CNY", "USD", "EUR", "JPY", "GBP", "HKD", "KRW",
                         "SGD", "AUD", "CAD", "CHF", "INR", "TWD", "RUB", "BRL"]
        if currency_var.get() not in currency_list:
            currency_list.insert(0, currency_var.get())
        cur_combo = ttk.Combobox(frame, textvariable=currency_var, values=currency_list,
                                  state="readonly", width=14)
        cur_combo.pack(anchor="w", pady=(0, 2))
        ttk.Label(frame, text=T("currency_hint", lang), foreground="gray").pack(
            anchor="w", pady=(0, 10))

        # --- Language ---
        ttk.Label(frame, text=T("language_label", lang)).pack(anchor="w", pady=(2, 0))
        LANG_OPTIONS = {"中文": "zh", "English": "en"}
        LANG_DISPLAY = list(LANG_OPTIONS.keys())
        cur_lang_display = {v: k for k, v in LANG_OPTIONS.items()}.get(
            app.config.get("language", "zh"), "中文")
        lang_var = tk.StringVar(value=cur_lang_display)
        lang_combo = ttk.Combobox(frame, textvariable=lang_var, values=LANG_DISPLAY,
                                  state="readonly", width=14)
        lang_combo.pack(anchor="w", pady=(0, 12))

        # --- Auto-Start ---
        from src.app_state import get_auto_start_state
        auto_start_var = tk.BooleanVar(
            value=app.config.get("auto_start", False) or get_auto_start_state())
        ttk.Checkbutton(frame, text=T("auto_start_label", lang),
                        variable=auto_start_var).pack(anchor="w", pady=(0, 12))

        ttk.Separator(frame, orient="horizontal").pack(fill="x", pady=5)

        # --- Status ---
        with app._lock:
            last = app.last_check

        if last:
            last_str = last.strftime("%Y-%m-%d %H:%M:%S")
        else:
            last_str = T("not_checked", lang)

        b = app.get_preferred_balance()
        if b:
            sym = currency_sym(b["currency"])
            status = T("status_line", lang, last=last_str, sym=sym,
                       total=f"{b['total_balance']:,.2f}")
        else:
            status = T("status_line_no", lang, last=last_str)
        ttk.Label(frame, text=status, foreground="gray").pack(anchor="w", pady=(8, 12))

        # --- Buttons ---
        btn_frame = ttk.Frame(frame)
        btn_frame.pack(fill="x")

        def on_save():
            # Lazy import to avoid circular dependency with tray_app
            from src.tray_app import do_balance_check

            key = api_var.get().strip()
            if not key:
                messagebox.showwarning(T("warn_title", lang), T("warn_no_key", lang),
                                       parent=root)
                return
            app.config["api_key"] = key
            app.config["interval_minutes"] = interval_var.get()
            app.config["threshold_yuan"] = threshold_var.get()
            app.config["language"] = LANG_OPTIONS.get(lang_var.get(), "zh")
            app.config["preferred_currency"] = currency_var.get()
            app.config["auto_start"] = auto_start_var.get()
            # Apply auto-start immediately
            from src.app_state import set_auto_start
            set_auto_start(app.config["auto_start"])
            save_config(app.config)
            app.cancel_timer()
            # Language change: menu updates on next app launch;
            # settings dialog itself refreshes on next open.
            threading.Thread(target=do_balance_check, args=(app,), daemon=True).start()
            log("Settings saved")
            root.destroy()

        ttk.Button(btn_frame, text=T("save", lang), command=on_save).pack(
            side="right", padx=(5, 0))
        ttk.Button(btn_frame, text=T("cancel", lang), command=root.destroy).pack(
            side="right")
        root.bind("<Return>", lambda e: on_save())
        root.bind("<Escape>", lambda e: root.destroy())
        api_entry.focus_set()
        root.mainloop()

    _dialog()
