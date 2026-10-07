import {Env} from "../../worker";
import {getAuthUser} from "../../auth";
import {InvoicesBackfillStatus} from "../../types/invoices";
import {D1Driver, Repository} from "../../repository/d1";

let repo: Repository;
const getRepo = (env: Env) => repo ??= new Repository(new D1Driver(env.D1));

export async function start(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const backfillStatus = await getRepo(env).get<InvoicesBackfillStatus>("invoices_backfill", {ownerId: appUser.id});
    if (backfillStatus)
        return Response.json({ success: true, existing: true }, { status: 202 });
    const createdBackfillStatus = await getRepo(env).save("invoices_backfill", { ownerId: appUser.id, status: "queued" }, true);
    if (!createdBackfillStatus.changes)
        return Response.json({ success: true, existing: true }, { status: 202 });
    try {
        await env.BACKFILL_JOB.create({ id: appUser.id, params: { userId: appUser.id } });
    } catch (error) {
        await getRepo(env).update("invoices_backfill", { status: "failed", error: String(error), updatedAt: new Date().toISOString()}, { ownerId: appUser.id });
        return Response.json({ success: false, error: String(error) }, { status: 502 });
    }
    return Response.json({ success: true, existing: false }, { status: 202 });
}

export async function status(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const invoicesBackfill = await getRepo(env).get<InvoicesBackfillStatus>("invoices_backfill", {ownerId: appUser.id});
    if (!invoicesBackfill)
        return Response.json({ success: false, error: "Backfill job not found" }, { status: 404 });
    const job = await refreshJob(env, invoicesBackfill);
    return Response.json({ success: true, ...job });
}

async function refreshJob(env: Env, job: InvoicesBackfillStatus) {
    if (job.status !== "queued" && job.status !== "running") return job;
    const execution = await env.BACKFILL_JOB.get(job.ownerId).then(instance => instance.status());
    if (execution.status !== "errored" && execution.status !== "terminated" && execution.status !== "complete") return job;
    const status = execution.status === "complete" ? "completed" : "failed";
    const error = execution.error?.message;
    await getRepo(env).update("invoices_backfill", { status, error: error, updatedAt: new Date().toISOString() }, { ownerId: job.ownerId });
    return {...job, status, error};
}
