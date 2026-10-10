import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {AppInvoice} from "../../types/invoices";
import {getAuthUser} from "../../auth";
import {invoiceFromXml} from "../../service/invoices";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

/// Months with at least one sales invoice.
export async function months(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const year = new URL(req.url).searchParams.get("year") ?? "";
    if (!/^\d{4}$/.test(year) || Number(year) === 0)
        return Response.json({ success: false, error: "Invalid year" }, { status: 400 });
    const range = { field: "issueDate", start: `${year}-01-01`, end: `${year}-12-31`, startInclusive: true, endInclusive: true };
    const invoices = await getRepo(env).getAll<Pick<AppInvoice, "issueDate">>("invoices", { ownerId: appUser.id, type: "sales" }, range, ["issueDate"]);
    const months = Array<boolean>(12).fill(false);
    for (const invoice of invoices) months[Number(invoice.issueDate.slice(5, 7)) - 1] = true;
    return Response.json(months);
}

export async function get(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const filters = Object.fromEntries([...url.searchParams].filter(([key]) => key === "id" || key === "type"));
    if (appUser.tier !== 0) filters.ownerId = appUser.id;
    // Query by dates range
    const fromDate = url.searchParams.has("from") ? new Date(url.searchParams.get("from")!) : undefined;
    const toDate = url.searchParams.has("to") ? new Date(url.searchParams.get("to")!) : undefined;
    if ((fromDate && isNaN(fromDate.getTime())) || (toDate && isNaN(toDate.getTime())) || (fromDate && toDate && fromDate > toDate))
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    const range = { field: "issueDate", start: fromDate?.toISOString().slice(0, 10), end: toDate?.toISOString().slice(0, 10) };
    const result = await getRepo(env).getAll<AppInvoice & { ownerId: string }>("invoices", filters, range);
    return result.length === 0
        ? Response.json({ success: false, error: "No invoice found", filters }, { status: 404 })
        : Response.json(result, { status: 200 });
}

export async function post(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const form = await req.formData();
    const file = form.get("file");
    if (!(file instanceof File)) return Response.json({ error: "Missing XML file" }, { status: 400 });
    const type = form.get("type")!.toString() as "purchase" | "sales";
    const notes = form.get("notes")?.toString();
    const record = await invoiceFromXml(env, await file.text(), appUser, type, notes);
    try {
        await getRepo(env).save<AppInvoice>("invoices", record);
        return Response.json({ success: true, id: record.id }, { status: 201 });
    } catch (error) {
        if (String(error).includes("UNIQUE constraint failed"))
            return Response.json({ success: false, error: "Invoice already exists", id: record.id }, { status: 409 });
        else
            throw error;
    }
}

export async function put(_req: Request, _env: Env): Promise<Response> {
    return Response.json({ error: "Updating invoices is forbidden" }, { status: 405 });
}

export async function del(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    // Allow to delete only owned invoices (except for superadmin)
    const filters: Record<string, any> = {};
    for (const [key, value] of url.searchParams.entries()) {
        filters[key] = value;
    }
    if (appUser.tier !== 0) filters.ownerId = appUser.id;
    const result = await getRepo(env).delete("invoices", filters);
    return result.changes === 0
        ? Response.json({ success: false, error: "No invoice found", filters }, { status: 404 })
        : Response.json({ success: result.success, changes: result.changes, ...filters }, { status: 200 });
}