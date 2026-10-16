import {Env} from "../../worker";
import {AppUser} from "../../types/users";
import {InvoicesPeriodsSynced} from "../../types/invoices";
import {KsefClient} from "../../clients/ksef";
import {invoiceFromXml} from "./invoices";
import {D1Driver, Repository} from "../../repository/d1";

let repo: Repository;
const getRepo = (env: Env) => repo ??= new Repository(new D1Driver(env.D1));

export async function syncInvoicesWithKsef(env: Env, appUser: AppUser, type: "sales" | "purchase", from: string, to: string) {
    const uncoveredPeriods = await findUncoveredPeriods(env, appUser.id, type, from, to);
    let count = 0;
    let inserted = 0;
    for (const period of uncoveredPeriods) {
        const fromDate = new Date(`${period.from}T00:00:00.000Z`);
        const toDate = new Date(`${period.to}T23:59:59.999Z`);
        const result = await fetchFromKsefForPeriod(env, appUser, type, fromDate, toDate);
        count += result.count;
        inserted += result.inserted;
    }
    return { count, inserted };
}

async function findUncoveredPeriods(env: Env, ownerId: string, type: "sales" | "purchase", from: string, to: string) {
    let syncedPeriods: { dateFrom: string; dateTo: string }[] = [];
    if (type === "sales")
        syncedPeriods = await getRepo(env).getAll<Pick<InvoicesPeriodsSynced, "dateFrom" | "dateTo">>("invoices_periods_synced", {ownerId, type}, undefined, ["dateFrom", "dateTo"]);
    const overlappingSyncedPeriods = syncedPeriods
        .filter(period => period.dateTo >= from && period.dateFrom <= to)
        .sort((a, b) => a.dateFrom.localeCompare(b.dateFrom));
    const millisecondsPerDay = 86_400_000;
    const requestedEnd = Date.parse(`${to}T00:00:00.000Z`);
    let nextDate = Date.parse(`${from}T00:00:00.000Z`);
    const uncoveredPeriods: { from: string; to: string }[] = [];
    for (const syncedPeriod of overlappingSyncedPeriods) {
        const syncedFrom = Date.parse(`${syncedPeriod.dateFrom}T00:00:00.000Z`);
        const syncedTo = Date.parse(`${syncedPeriod.dateTo}T00:00:00.000Z`);
        if (syncedTo < nextDate) continue;
        if (syncedFrom > nextDate)
            uncoveredPeriods.push({ from: new Date(nextDate).toISOString().slice(0, 10), to: new Date(Math.min(requestedEnd, syncedFrom - millisecondsPerDay)).toISOString().slice(0, 10) });
        nextDate = Math.max(nextDate, syncedTo + millisecondsPerDay);
        if (nextDate > requestedEnd) break;
    }
    if (nextDate <= requestedEnd)
        uncoveredPeriods.push({ from: new Date(nextDate).toISOString().slice(0, 10), to });
    return uncoveredPeriods;
}

async function fetchFromKsefForPeriod(env: Env, appUser: AppUser, type: "sales" | "purchase", from: Date, to: Date) {
    const client = new KsefClient(env);
    const invoiceMetadata = await client.queryInvoiceMetadata(appUser, type, undefined, from, to);
    let inserted = 0;
    for (let batchStart = 0; batchStart < invoiceMetadata.invoices.length; batchStart += 20) {
        const invoices = await Promise.all(invoiceMetadata.invoices.slice(batchStart, batchStart + 20).map(async invoiceMetadata => {
            const xml = await client.downloadInvoice(invoiceMetadata.ksefNumber);
            return invoiceFromXml(env, xml, appUser, type, "Downloaded from KSeF");
        }));
        for (const invoice of invoices)
            inserted += (await getRepo(env).save("invoices", invoice, true)).changes;
    }
    if (type === "sales")
        await recordSyncedInvoicePeriod(env, appUser.id, "sales", from.toISOString().slice(0, 10), to.toISOString().slice(0, 10));
    return { count: invoiceMetadata.invoices.length, inserted };
}

export async function recordSyncedInvoicePeriod(env: Env, ownerId: string, type: "sales", from: string, to: string) {
    const periodKey = {ownerId, type, dateFrom: from, dateTo: to};
    const updatedAt = new Date().toISOString();
    await getRepo(env).save("invoices_periods_synced", {...periodKey, updatedAt}, true);
    await getRepo(env).update("invoices_periods_synced", {updatedAt}, periodKey);
}
