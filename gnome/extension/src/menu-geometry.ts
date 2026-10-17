export function menuWidth(monitorWidth: number, maximum: number): number {
    return Math.min(monitorWidth - 32, maximum);
}

export function statusBudget(monitorWidth: number, occupiedWidth: number): number {
    return Math.max(0, Math.min(Math.floor(monitorWidth * 0.35), monitorWidth - occupiedWidth - 16));
}
