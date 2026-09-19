import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {getAuthUser} from "../../auth";
import {nanoid} from "nanoid";
import {AppContractor, AppContractorUpdate} from "../../types/contractors";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

export async function get(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    // Allow to fetch only owned contractors (except for superadmin)
    const filters: Record<string, any> = appUser.tier === 0 ? {} : { ownerId: appUser.id };
    for (const [key, value] of url.searchParams.entries()) {
        filters[key] = value;
    }
    const rows = Object.keys(filters).length
        ? await getRepo(env).get<AppContractor>("contractors", filters)
        : await getRepo(env).getAll<AppContractor>("contractors");
    if (!rows)
        return Response.json({ success: false, error: "Contractor not found", filters }, { status: 404 });
    return Response.json(rows, { status: 200 });
}

export async function post(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const payload = await req.json() as AppContractor;
    // Never allow client to control id, ownership, or creation/update timestamps
    const { id, createdAt, updatedAt, ...payloadData } = payload;
    const record = { ...payloadData, id: nanoid(), ownerId: appUser.id, updatedAt: new Date().toISOString() };
    try {
        await getRepo(env).save<AppContractor>("contractors", record);
        return Response.json({ success: true, id: record.id }, { status: 201 });
    } catch (error) {
        if (String(error).includes("UNIQUE constraint failed"))
            return Response.json({ success: false, error: "Contractor already exists", id: record.id }, { status: 409 });
        throw error;
    }
}

export async function put(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const payload = await req.json() as AppContractor & { ownerId?: string };
    // Never allow client to change id, ownership, or creation/update timestamp
    const { id, ownerId, createdAt, updatedAt, ...updatePayload } = payload;
    const result = await getRepo(env).update<AppContractorUpdate>("contractors", {
        ...updatePayload,
        updatedAt: new Date().toISOString()
    }, { id, ownerId: appUser.id });
    if (result.changes === 0)
        return Response.json({ success: false, error: "Contractor not found", id: id }, { status: 404 });
    return Response.json({ success: true, changes: result.changes, id: id }, { status: result.success ? 200 : 400 });
}

export async function del(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    // Allow to delete only owned contractors (except for superadmin)
    const filters: Record<string, any> = {};
    for (const [key, value] of url.searchParams.entries()) {
        filters[key] = value;
    }
    if (appUser.tier !== 0) filters.ownerId = appUser.id;
    const result = await getRepo(env).delete("contractors", filters);
    if (result.changes === 0)
        return Response.json({ success: false, error: "Contractor not found", filters }, { status: 404 });
    return Response.json({ success: result.success, changes: result.changes, ...filters }, { status: 200 });
}