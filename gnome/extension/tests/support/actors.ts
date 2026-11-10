import type Clutter from 'gi://Clutter';
import type Gio from 'gi://Gio';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import type {QuickMenuToggle} from 'resource:///org/gnome/shell/ui/quickSettings.js';
import type {Indicator} from '../../src/panel.js';

export interface TestActor extends Clutter.Actor {
    text?: string;
    title?: string;
    checked?: boolean;
    hint_text?: string;
    gicon?: Gio.Icon;
    menu?: QuickMenuToggle['menu'];
    get_label?: St.Button['get_label'];
    get_text?: St.Entry['get_text'];
    set_text?: St.Entry['set_text'];
    has_style_class_name?: St.Widget['has_style_class_name'];
    has_style_pseudo_class?: St.Widget['has_style_pseudo_class'];
    get_children(): TestActor[];
}

export function findActor(
    root: Clutter.Actor | null | undefined,
    predicate: (actor: TestActor) => unknown,
): TestActor | null;
export function findActor<Value extends Clutter.Actor>(
    root: Clutter.Actor | null | undefined,
    predicate: (actor: TestActor) => unknown,
    kind: abstract new (...args: never[]) => Value,
): (Value & TestActor) | null;
export function findActor(
    root: Clutter.Actor | null | undefined,
    predicate: (actor: TestActor) => unknown,
    kind?: abstract new (...args: never[]) => Clutter.Actor,
): TestActor | null {
    if (!root) return null;
    if (predicate(root)) return kind ? instance(root, kind) : root;
    for (const child of root.get_children()) {
        const found = kind ? findActor(child, predicate, kind) : findActor(child, predicate);
        if (found) return found;
    }
    return null;
}

export function findActors(root: Clutter.Actor, predicate: (actor: TestActor) => unknown): TestActor[] {
    return [...(predicate(root) ? [root] : []), ...root.get_children().flatMap(child => findActors(child, predicate))];
}

export function containsText(root: Clutter.Actor | null | undefined, expected: string): boolean {
    return findActor(root, actor => (actor.text ?? actor.get_label?.() ?? '').includes(expected)) !== null;
}

export function required<Value>(value: Value | null | undefined): NonNullable<Value> {
    if (value === null || value === undefined) throw new Error('Expected a test value');
    return value;
}

export function instance<Value>(value: unknown, kind: abstract new (...args: never[]) => Value): Value {
    if (!(value instanceof kind)) throw new Error(`Expected ${kind.name}`);
    return value;
}

export function optionalInstance<Value>(
    value: unknown,
    kind: abstract new (...args: never[]) => Value,
): Value | undefined {
    return value instanceof kind ? value : undefined;
}

export function contentOf(actor: Clutter.Actor | null | undefined): TestActor {
    return instance((actor instanceof St.ScrollView ? actor : instance(actor, St.Bin)).get_child(), St.Widget);
}

export function indicator(): Indicator | undefined {
    return Main.panel.statusArea['quotaline@bernat.tech'] as Indicator | undefined;
}

// Clutter and St return null for unset names and labels despite their introspection declarations.
export function accessibleName(actor: Clutter.Actor | null | undefined): string | null {
    return (actor?.accessible_name as string | null | undefined) ?? null;
}

export function buttonLabel(actor: Clutter.Actor | null | undefined): string | null {
    return actor instanceof St.Button ? actor.get_label() : null;
}

export function clickButton(actor: Clutter.Actor): void {
    instance(actor, St.Button).emit('clicked', 1);
}

export function failure(error: unknown): Error {
    return error instanceof Error ? error : new Error(String(error));
}
