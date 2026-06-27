local client = vim.lsp.start_client {
    name = "btls",
    cmd = { "./target/debug/btls" },
    settings = {
        btls = {
            diagnostics = true,
        }
    }
}

vim.filetype.add({
    extension = {
        bt = "bpftrace"
    }
})

if not client then
    vim.notify "client thing no good"
    return
end

vim.api.nvim_create_autocmd("FileType", {
    pattern = "bpftrace",
    callback = function(event)
        vim.lsp.buf_attach_client(event.buf, client)
        vim.keymap.set('n', 'gd', vim.lsp.buf.definition, { buffer = event.buf })
    end
})

