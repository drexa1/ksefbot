import {Env} from "../../worker";
import {corsHeaders, getAuthUser} from "../../auth";
import {service} from "../../services/services";
import {AppUser} from "../../types/users";

export async function get(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    // Allow to fetch users only to superadmin
    if (appUser.tier !== 0)
        return new Response("Unauthorized", {status: 401, headers: corsHeaders});
    const url = new URL(req.url);
    const filters = Object.fromEntries(url.searchParams.entries());
    const rows = await service.users.get(filters);
    return !rows || rows.length === 0
        ? Response.json({success: false, error: "No users found", filters: filters}, {status: 404})
        : Response.json(rows, {status: 200});
}

/**
 * 🐣 User creation triggered by the onboarding flow.
 */
export async function post(req: Request): Promise<Response> {
    // 👀 At this point there is no user to verify yet
    const payload = await req.json() as AppUser;
    try {
        const result = await service.users.create(payload);
        return Response.json({success: true, id: result.id}, {status: 201});
    } catch (error) {
        if (String(error).includes("UNIQUE constraint failed"))
            return Response.json({success: false, error: "User already exists", id: payload.id}, {status: 409});
        throw error;
    }
}

export async function put(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    // Allow to update users only to superadmin
    if (appUser.tier !== 0)
        return new Response("Unauthorized", {status: 401, headers: corsHeaders});
    const payload = await req.json() as AppUser;
    const result = await service.users.update(payload);
    return result.changes === 0
        ? Response.json({success: false, error: "No users found", id: result.id}, {status: 404})
        : Response.json({success: true, changes: result.changes, id: result.id}, {status: result.success ? 200 : 400});
}

export async function del(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    // Only superadmins can delete users
    if (appUser.tier !== 0)
        return new Response("Unauthorized", {status: 401, headers: corsHeaders});
    const url = new URL(req.url);
    const filters = Object.fromEntries(url.searchParams.entries());
    const result = await service.users.delete(filters);
    return result.changes === 0
        ? Response.json({success: false, error: "No users found", filters}, {status: 404})
        : Response.json({success: result.success, changes: result.changes, ...filters}, {status: 200});
}