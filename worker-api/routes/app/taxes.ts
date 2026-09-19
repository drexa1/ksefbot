import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {getAuthUser} from "../../auth";
import {fetchKsefInvoices} from "../ksef/ksef";
import {AppTaxRecord, AppTaxRecordDb, TaxRecordObligations} from "../../types/taxes";
import {AppUser} from "../../types/users";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

export async function get(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const fromParam = url.searchParams.get("from");
    const toParam = url.searchParams.get("to");
    const from = fromParam ? new Date(fromParam) : null;
    const to = toParam ? new Date(toParam) : null;
    if ((fromParam && !toParam) || (!fromParam && toParam))
        return Response.json({ success: false, error: "From and To are required together" }, { status: 400 });
    if (from && to && (isNaN(from.getTime()) || isNaN(to.getTime()) || from > to))
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    // Allow to fetch only owned tax records (except for superadmin)
    const filters = appUser.tier === 0 ? {} : { ownerId: appUser.id };
    const rows = from && to
        ? await getRepo(env).get<AppTaxRecordDb>("taxes", { from: from.toISOString(), to: to.toISOString(), ...filters })
        : await getRepo(env).getAll<AppTaxRecordDb>("taxes", filters);
    if (!rows)
        return Response.json({ success: false, error: "Tax records not found", from: from, to: to }, { status: 404 });
    const result = Array.isArray(rows)
        ? rows.map(row => ({ ...row, purchasesSummary: JSON.parse(row.purchasesSummary) }))
        : { ...rows, purchasesSummary: JSON.parse(rows.purchasesSummary) };
    return Response.json(result, { status: 200 });
}

export async function simulate(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const hoursWorked = Number(url.searchParams.get("hoursWorked"));
    if (!Number.isInteger(hoursWorked) || hoursWorked < 0)
        return Response.json({ success: false, error: "Invalid number of hours worked" }, { status: 400 });
    if (appUser.defaultHourlyRate == null)
        return Response.json({ success: false, error: "Default hourly rate is not configured" }, { status: 400 });
    const now = new Date();
    const from = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1));
    const to = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1));
    const brutIncome = Number((appUser.defaultHourlyRate *  hoursWorked).toFixed(2));
    const taxRecord = { from: from.toISOString(), to: to.toISOString(), brutIncome } as AppTaxRecord;
    const obligations = await computeObligations(env, appUser, taxRecord, from, to);
    const totalCleanRevenue = Number((obligations.netBeforeObligations - obligations.incomeTax - obligations.healthContribution + obligations.purchasesDeductions).toFixed(2));
    return Response.json({
        hoursWorked: hoursWorked,
        hourlyRate: appUser.defaultHourlyRate,
        brutIncome: brutIncome,
        ...obligations,
        totalCleanRevenue: totalCleanRevenue
    });
}

export async function post(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const payload = await req.json() as AppTaxRecord;
    const from = new Date(payload.from);
    const to = new Date(payload.to);
    if (isNaN(from.getTime()) || isNaN(to.getTime()) || from > to)
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    // Never allow client to control id, ownership, or creation/update timestamps
    const { createdAt, updatedAt, ...payloadData } = payload;
    const obligations = await computeObligations(env, appUser, payloadData, from, to);
    const record = {
        ...payloadData,
        from: from.toISOString(),
        to: to.toISOString(),
        ownerId: appUser.id,
        vatPercentage: obligations.vatPercentage,
        vatAmount: obligations.vatAmount,
        netBeforeObligations: obligations.netBeforeObligations,
        taxRate: obligations.taxRate,
        incomeTax: obligations.incomeTax,
        healthInsuranceBase: obligations.healthInsuranceBase,
        healthInsuranceRate: obligations.healthInsuranceRate,
        healthContribution: obligations.healthContribution,
        purchasesSummary: JSON.stringify(obligations.purchasesSummary),
        totalCleanRevenue: Number(((obligations.netBeforeObligations - obligations.incomeTax - obligations.healthContribution) + obligations.purchasesDeductions).toFixed(2)),
        ...(payload.notes && { notes: payload.notes }),
        updatedAt: new Date().toISOString()
    };
    try {
        await getRepo(env).save<AppTaxRecordDb>("taxes", record);
        return Response.json({ success: true, from: record.from, to: record.to }, { status: 201 });
    } catch (error) {
        if (String(error).includes("UNIQUE constraint failed"))
            return Response.json({ success: false, error: "Tax record already exists", from: record.from, to: record.to }, { status: 409 });
        throw error;
    }
}

async function computeObligations(env: Env, appUser: AppUser, taxRecord: AppTaxRecord, from: Date, to: Date): Promise<TaxRecordObligations> {
    // VAT
    const vatPercentage = taxRecord.vatPercentage ?? env.DEFAULT_VAT_PERCENTAGE;
    const vatAmount = Number((taxRecord.brutIncome * vatPercentage / (100 + vatPercentage)).toFixed(2));
    const netBeforeObligations = Number((taxRecord.brutIncome - vatAmount).toFixed(2));
    // Obligations
    const taxRate = taxRecord.taxRate ?? env.DEFAULT_TAX_RATE;
    const incomeTax = Number((netBeforeObligations * taxRate / 100).toFixed(2));
    const healthInsuranceBase = taxRecord.healthInsuranceBase ?? env.DEFAULT_HEALTH_INSURANCE_BASE;
    const healthInsuranceRate = taxRecord.healthInsuranceRate ?? env.DEFAULT_HEALTH_INSURANCE_RATE;
    const healthContribution = Number((healthInsuranceBase * healthInsuranceRate / 100).toFixed(2));
    // Purchases deductions
    const purchasesInvoices = env.TEST_MODE ? [] : await fetchKsefInvoices(env, appUser, "Subject2", from, to);
    const purchasesSummary = purchasesInvoices.map((invoice) => ({
        InvoiceNumber: invoice.InvoiceBody?.InvoiceNumber,
        TotalGrossAmount: invoice.InvoiceBody?.TotalGrossAmount,
        TotalVatAmount: invoice.InvoiceBody?.TotalVatAmount
    }));
    const purchasesDeductions = Number(purchasesInvoices.reduce((sum, invoice) => sum + (invoice.InvoiceBody?.TotalVatAmount ?? 0), 0).toFixed(2));
    // Total after obligations and purchases deductions
    return {
        vatPercentage,
        vatAmount,
        netBeforeObligations,
        taxRate,
        incomeTax,
        healthInsuranceBase,
        healthInsuranceRate,
        healthContribution,
        purchasesDeductions,
        purchasesSummary,
    };
}

export async function put(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const payload = await req.json() as AppTaxRecord & { ownerId?: string };
    const from = new Date(payload.from);
    const to = new Date(payload.to);
    if (isNaN(from.getTime()) || isNaN(to.getTime()) || from > to)
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    // Never allow client to change id, ownership, or creation/update timestamp
    const { ownerId, createdAt, updatedAt, ...updatePayload } = payload;
    const result = await getRepo(env).update<AppTaxRecord>("taxes", {
        ...updatePayload,
        from: from.toISOString(),
        to: to.toISOString(),
        updatedAt: new Date().toISOString()
    }, { ownerId: appUser.id });
    if (result.changes === 0)
        return Response.json({ success: false, error: "Tax record not found", from: from, to: to }, { status: 404 });
    return Response.json({ success: true, changes: result.changes, from: from, to: to }, { status: result.success ? 200 : 400 });
}

export async function del(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const fromParam = url.searchParams.get("from")!;
    const toParam = url.searchParams.get("to")!;
    const from = new Date(fromParam);
    const to = new Date(toParam);
    if (isNaN(from.getTime()) || isNaN(to.getTime()) || from > to)
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    // Allow to delete only owned tax records (except for superadmin)
    const filters = appUser.tier === 0 ? {} : { ownerId: appUser.id };
    const result = await getRepo(env).delete("taxes", { from: from.toISOString(), to: to.toISOString(), ...filters });
    if (result.changes === 0)
        return Response.json({ success: false, error: "Tax records not found", from: from, to: to }, { status: 404 });
    return Response.json({ success: result.success, changes: result.changes, from: from, to: to }, { status: 200 });
}