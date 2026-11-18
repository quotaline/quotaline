// GNOME 50 reads animation flags, while GNOME 51 reads an options object and animates when handed anything else.
export function noAnimation<Flags>(shellVersion: string, none: Flags): Flags | {animate: false} {
    return Number.parseInt(shellVersion, 10) >= 51 ? {animate: false} : none;
}
