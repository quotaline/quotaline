import type GLib from 'gi://GLib';
import type GObject from 'gi://GObject';
import type St from 'gi://St';

// GNOME Shell registers these signals in JavaScript, outside generated types.
declare module 'resource:///org/gnome/shell/ui/layout.js' {
    interface LayoutManager {
        connect(signal: 'monitors-changed', callback: () => void): number;
        connect<K extends keyof GObject.Object.SignalSignatures>(
            signal: K,
            callback: GObject.SignalCallback<this, GObject.Object.SignalSignatures[K]>,
        ): number;
        emit(signal: 'monitors-changed'): void;
        emit<K extends keyof GObject.Object.SignalSignatures>(
            signal: K,
            ...args: GObject.GjsParameters<GObject.Object.SignalSignatures[K]>
        ): void;
    }
}

declare module 'resource:///org/gnome/shell/ui/calendar.js' {
    interface Calendar {
        connect(signal: 'selected-date-changed', callback: (calendar: Calendar, date: GLib.DateTime) => void): number;
        connect<K extends keyof St.Widget.SignalSignatures>(
            signal: K,
            callback: GObject.SignalCallback<this, St.Widget.SignalSignatures[K]>,
        ): number;
    }
}
