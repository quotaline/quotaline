import type GObject from 'gi://GObject';
import type {Calendar} from '@girs/gnome-shell/ui/calendar';

declare module 'resource:///org/gnome/shell/ui/calendar.js' {
    // Shell exports this base, whose members throw until overridden, but the @girs typings keep it module-private.
    type EventSource = Parameters<Calendar['setEventSource']>[0];

    export class EventSourceBase extends GObject.Object {
        readonly isLoading: boolean;
        readonly hasCalendars: boolean;
        destroy(): void;
        requestRange(begin: Date, end: Date): void;
        getEvents(begin: Date, end: Date): ReturnType<EventSource['getEvents']>;
        hasEvents(day: Date): boolean;
    }
}
