export async function get(): Promise<Response> {
    return Response.json("OK", { status: 200 });
}