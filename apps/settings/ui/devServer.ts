/** The dev server's port. `SNAPPY_UI_PORT` gives a test run a server of its own, apart from another checkout's. */
export const DEV_PORT = Number(process.env.SNAPPY_UI_PORT ?? 5179);
