local client = vim.lsp.start_client {
    name = "btls",
    cmd = { "./target/debug/btls" }
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
    callback = function()
        vim.lsp.buf_attach_client(0, client)
    end
})

