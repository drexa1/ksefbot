export const scalarHtml = `
    <!doctype html>
    <html lang="en">
        <head>
            <title>API docs</title>
            <!--suppress HtmlUnknownTarget -->
            <link rel="icon" href="favicon.png">
        </head>
        <body>
            <div id="app"></div>
            <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference/dist/browser/standalone.min.js"></script>
            <script>
                // noinspection JSUnresolvedVariable
                Scalar.createApiReference("#app", {
                    url: "/openapi.json",
                    theme: "laserwave",
                    orderSchemaPropertiesBy: "preserve",
                    defaultOpenAllTags: false,
                    hideSearch: true
                });
                document.querySelector("body").classList.add("light-mode");
                document.querySelector("body").classList.add("dark-mode");
                // Collapse sections
                const observer = new MutationObserver(() => {
                    document.querySelectorAll('button[aria-expanded="true"]').forEach(btn => btn.setAttribute('aria-expanded', 'false'));
                    observer.disconnect();
                });
                observer.observe(document.getElementById("app"), { childList: true, subtree: true });
            </script>
        </body>
    </html>
`;
