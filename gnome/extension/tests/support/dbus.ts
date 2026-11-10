import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import type {ServiceState} from '../../src/service-state.js';
import {required} from './actors.js';

export type ProxyCall = (
    this: Gio.DBusProxy,
    method: string,
    parameters: GLib.Variant | null,
    flags: Gio.DBusCallFlags,
    timeout: number,
    cancellable: Gio.Cancellable | null,
    callback: (source: Gio.DBusProxy | null, result: Gio.AsyncResult) => void,
) => void;

// These scenarios exercise the callback API; the application does not promisify it.
export const proxyCalls: {
    call: ProxyCall;
    call_finish: Gio.DBusProxy['call_finish'];
    connect: Gio.DBusProxy['connect'];
} = Gio.DBusProxy.prototype;

export const sessionCalls: {
    call: (
        this: Gio.DBusConnection,
        bus: string | null,
        path: string,
        iface: string,
        method: string,
        parameters: GLib.Variant | null,
        reply: GLib.VariantType | null,
        flags: Gio.DBusCallFlags,
        timeout: number,
        cancellable: Gio.Cancellable | null,
        callback: (source: Gio.DBusConnection | null, result: Gio.AsyncResult) => void,
    ) => void;
} = Gio.DBus.session;

export const proxyFactory: {
    new_for_bus: (
        bus: Gio.BusType,
        flags: Gio.DBusProxyFlags,
        info: Gio.DBusInterfaceInfo | null,
        name: string,
        path: string,
        iface: string,
        cancellable: Gio.Cancellable | null,
        callback: (source: Gio.DBusProxy | null, result: Gio.AsyncResult) => void,
    ) => void;
} = Gio.DBusProxy;

export function readState(): ServiceState {
    const [json] = Gio.DBus.session
        .call_sync(
            'tech.bernat.Quotaline.Gnome',
            '/tech/bernat/Quotaline/Gnome',
            'tech.bernat.Quotaline.Gnome',
            'GetState',
            null,
            new GLib.VariantType('(s)'),
            Gio.DBusCallFlags.NONE,
            2000,
            null,
        )
        .deep_unpack() as [string];
    return JSON.parse(json) as ServiceState;
}

export function readProvider(id: string) {
    return required(readState().providers[id]);
}

export function readProviderSettings(id: string) {
    const {providers, interval_minutes} = readState().settings;
    return {enabled: providers[id], intervalMinutes: interval_minutes[id]};
}
