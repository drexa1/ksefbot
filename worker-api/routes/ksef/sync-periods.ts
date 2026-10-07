import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {InvoicesSyncPeriod} from "../../types/invoices";

const msPerDay = 86_400_000;
const periodTTL = 24 * 60 * 60 * 1000;  // ⌛ 24h TTL - is what a period holds up before being flagged outdated

let repo: Repository;
const getRepo = (env: Env) => repo ??= new Repository(new D1Driver(env.D1));

async function getSyncPeriods(type: "sales" | "purchase", env: Env, ownerId: string) {
    return (type !== "sales" ? [] : await getRepo(env).getAll<InvoicesSyncPeriod>("invoices_sync_periods", { ownerId, type}))
        .filter(syncedPeriod => Date.parse(syncedPeriod.updatedAt ?? "") >= Date.now() - periodTTL)
        .sort((p1, p2) => p1.dateFrom.localeCompare(p2.dateFrom));
}

export async function findUncoveredPeriods(env: Env, ownerId: string, type: "sales" | "purchase", from: Date, to: Date) {
    const uncoveredPeriods = [];
    let nextDate = from.getTime();
    for (const syncedPeriod of await getSyncPeriods(type, env, ownerId)) {
        const syncedFrom = Date.parse(syncedPeriod.dateFrom);
        const syncedTo = Date.parse(syncedPeriod.dateTo);
        if (syncedFrom > to.getTime()) break;
        if (syncedTo < nextDate) continue;
        if (syncedFrom > nextDate)
            uncoveredPeriods.push({ from: new Date(nextDate).toISOString().slice(0, 10), to: new Date(Math.min(to.getTime(), syncedFrom - msPerDay)).toISOString().slice(0, 10) });
        nextDate = Math.max(nextDate, syncedTo + msPerDay);
        if (nextDate > to.getTime()) break;
    }
    if (nextDate <= to.getTime()) uncoveredPeriods.push({ from: new Date(nextDate).toISOString().slice(0, 10), to: to.toISOString().slice(0, 10) });
    return uncoveredPeriods;
}

export async function saveSyncPeriod(env: Env, ownerId: string, type: "sales" | "purchase", from: string, to: string) {
    if (type !== "sales") return;  // For purchase invoices we enforce to always sync against KSeF
    const periodKey = { ownerId, type, dateFrom: from, dateTo: to };
    const updatedAt = new Date().toISOString();
    // Save new sync period or update existing one
    const saveResult = await getRepo(env).save("invoices_sync_periods", {...periodKey, updatedAt}, true);
    if (!saveResult.changes) await getRepo(env).update("invoices_sync_periods", {updatedAt}, periodKey);
}