import {Env} from "../../worker";
import {corsHeaders, getAuthUser} from "../../auth";
import {service} from "../../services/services";
import {AppContractor} from "../../types/contractors";

export async function get(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const searchParams = Object.fromEntries(url.searchParams.entries());
    const filters = appUser.tier === 0 ? searchParams : { ...searchParams, ownerId: appUser.id };
    const rows = await service.contractors.get(filters);
    return rows.length === 0
        ? Response.json({ success: false, error: "Contractor not found", filters: filters }, { status: 404 })
        : Response.json(rows, { status: 200 });
}

export async function post(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const payload = await req.json() as AppContractor;
    try {
        const result = await service.contractors.create(payload, appUser.id);
        return Response.json({ success: true, id: result.id }, { status: result.status });
    } catch (error) {
        if (String(error).includes("UNIQUE constraint failed"))
            return Response.json({ success: false, error: "Contractor already exists", id: payload.id }, { status: 409 });
        throw error;
    }
}

export async function put(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const payload = await req.json() as AppContractor & { ownerId?: string };
    const result = await service.contractors.update(payload, appUser.id);
    return result.changes === 0
        ? Response.json({ success: false, error: "No contractors found", id: result.id }, { status: 404 })
        : Response.json({ success: true, changes: result.changes, id: result.id }, { status: result.success ? 200 : 400 });
}

export async function del(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const filters = Object.fromEntries(url.searchParams.entries());
    const scopedFilters = appUser.tier === 0 ? filters : { ...filters, ownerId: appUser.id };
    const result = await service.contractors.delete(scopedFilters);
    return result.changes === 0
        ? Response.json({ success: false, error: "No contractors found", filters: result.filters }, { status: 404 })
        : Response.json({ success: result.success, changes: result.changes, ...result.filters }, { status: 200 });
}