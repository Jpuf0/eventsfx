import Gio from "gi://Gio";
import Glib from "gi://GLib";
import { Extension } from "resource:///org/gnome/shell/extensions/extension.js";

const BUS_NAME = "dev.eventsfx.Shell";
const OBJECT_PATH = "/dev/eventsfx/Shell";
const IFACE_XML = `
<node>
  <interface name="dev.eventsfx.Shell1">
    <property name="Fullscreen" type="b" access="read"/>
  </interface>
</node>`;

export default class EventsfxExtension extends Extension {
    enable() {
        this._fullscreen = false;
        this._window = null;
        this._windowSignal = 0;

        this._dbus = Gio.DBusExportedObject.wrapJSObject(IFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._nameId = Gio.bus_own_name_on_connection(
            Gio.DBus.session,
            BUS_NAME,
            Gio.BusNameOwnerFlags.None,
            null,
            null,
        );

        this._displaySignals = [
            global.display.connect("notify::focus-window", () =>
                this._trackFocus(),
            ),
            global.display.connect("in-fullscreen-changed", () =>
                this._update(),
            ),
        ];
        this._trackFocus();
    }

    disable() {
        this._set(false);

        for (const id of this._displaySignals) global.display.disconnect(id);

        this._displaySignals = [];
        this._untrackWindow();

        Gio.bus_unown_name(this._nameId);
        this._dbus.unexport();
        this._dbus = null;
    }

    get Fullscreen() {
        return this._fullscreen;
    }

    _trackFocus() {
        this._untrackWindow();
        this._window = global.display.focus_window;
        if (this._window) {
            this._windowSignal = this._window.connect(
                "notify::fullscreen",
                () => this._update(),
            );
        }
        this._update();
    }

    _untrackWindow() {
        if (this._window && this._windowSignal)
            this._window.disconnect(this._windowSignal);
        this._window = null;
        this._windowSignal = 0;
    }

    _update() {
        this._set(this._window?.is_fullscreen() ?? false);
    }

    _set(value) {
        if (value === this._fullscreen) return;
        this._fullscreen = value;
        this._dbus?.emit_property_changed(
            "Fullscreen",
            Glib.Variant.new_boolean(value),
        );
    }
}
