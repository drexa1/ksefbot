import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {InvoicesSyncPeriod} from "../../types/invoices";

const msPerDay = 86_400_000;
const msPerCoverageTtl = 24 * 60 * 60 * 1000;  // 24h TTL: skip rechecking periods during this window

let repo: Repository;
const getRepo = (env: Env) => repo ??= new Repository(new D1Driver(env.D1));

export async function findUncoveredPeriods(env: Env, ownerId: string, type: "sales" | "purchase", from: string, to: string) {
    const requestedEnd = Date.parse(to);
    let nextDate = Date.parse(from);
    const overlappingPeriods = (type !== "sales" ? [] : await getRepo(env).getAll<InvoicesSyncPeriod>("invoices_sync_periods", { ownerId, type }))
        .filter(syncedPeriod => Date.parse(syncedPeriod.updatedAt ?? "") >= Date.now() - msPerCoverageTtl)
        .filter(syncedPeriod => syncedPeriod.dateTo >= from && syncedPeriod.dateFrom <= to)
        .sort((p1, p2) => p1.dateFrom.localeCompare(p2.dateFrom));

    const uncoveredPeriods = [];
    for (const syncedPeriod of overlappingPeriods) {
        const syncedFrom = Date.parse(syncedPeriod.dateFrom);
        const syncedTo = Date.parse(syncedPeriod.dateTo);
        if (syncedTo < nextDate) continue;
        if (syncedFrom > nextDate)
            uncoveredPeriods.push({ from: new Date(nextDate).toISOString().slice(0, 10), to: new Date(Math.min(requestedEnd, syncedFrom - msPerDay)).toISOString().slice(0, 10) });
        nextDate = Math.max(nextDate, syncedTo + msPerDay);
        if (nextDate > requestedEnd) break;
    }
    if (nextDate <= requestedEnd)
        uncoveredPeriods.push({ from: new Date(nextDate).toISOString().slice(0, 10), to });
    return uncoveredPeriods;
}

export async function saveSyncPeriod(env: Env, ownerId: string, type: "sales", from: string, to: string) {
    const periodKey = { ownerId, type, dateFrom: from, dateTo: to };
    const updatedAt = new Date().toISOString();
    // Save new sync period or update existing one
    const saveResult = await getRepo(env).save("invoices_sync_periods", {...periodKey, updatedAt}, true);
    if (!saveResult.changes) await getRepo(env).update("invoices_sync_periods", {updatedAt}, periodKey);
}