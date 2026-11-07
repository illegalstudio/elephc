-- Purpose:
-- Drives a real nvim-dap session against `elephc debug --dap` and checks a
-- breakpoint, stack frame, local value, step and normal termination.
--
-- Called from:
-- - `scripts/smoke-nvim-dap.sh` (or manually with the same environment).
--
-- Key details:
-- - `ELEPHC_BIN` is the compiler binary, `ELEPHC_SMOKE_DIR` the project directory,
--   `ELEPHC_SMOKE_LINE` the breakpoint line, and `NVIM_DAP_DIR`/`NVIM_NIO_DIR`
--   the checked-out plugin directories.
-- - The fixture is expected to be the `run()` program created by the shell
--   script: breakpoint on `$x = add(2, 3)`, next line is the echo, so the first
--   stop has `$x = int(0)` and the step stop has `$x = int(5)`.
-- - The adapter is launched from a prebuilt debug binary (`main`) so nvim-dap's
--   adapter-start timeout measures the DAP handshake, not PHP compilation.

local function required_env(name)
  local value = vim.env[name]
  if not value or value == "" then
    error("missing environment variable " .. name)
  end
  return value
end

local elephc = required_env("ELEPHC_BIN")
local project = required_env("ELEPHC_SMOKE_DIR")
local break_line = tonumber(required_env("ELEPHC_SMOKE_LINE"))
local dap_dir = required_env("NVIM_DAP_DIR")
local nio_dir = required_env("NVIM_NIO_DIR")

vim.opt.runtimepath:append(dap_dir)
vim.opt.runtimepath:append(nio_dir)

local dap = require("dap")
dap.adapters.elephc = {
  type = "executable",
  command = elephc,
  args = { "debug", "--dap", "main" },
  options = { timeout = 30000 },
}
dap.configurations.php = {
  {
    type = "elephc",
    request = "launch",
    name = "elephc nvim-dap smoke",
    program = project .. "/main.php",
    stopOnEntry = false,
  },
}

vim.cmd("edit " .. vim.fn.fnameescape(project .. "/main.php"))
local bufnr = vim.api.nvim_get_current_buf()
require("dap.breakpoints").set({}, bufnr, break_line)

local stopped = 0
local done = false
local failed = nil
local saw_x_zero = false
local saw_x_five = false

local function fail(message)
  if not failed then
    failed = message
  end
  done = true
end

local function inspect_then_step(session, body)
  session:request("stackTrace", { threadId = body.threadId }, function(err, stack)
    if err then
      return fail("stackTrace: " .. vim.inspect(err))
    end
    local frame = stack.stackFrames[1]
    print(string.format("FRAME %s line=%d", frame.name, frame.line))
    if stopped == 1 and frame.name ~= "run" then
      return fail("first frame is " .. frame.name)
    end
    session:request("scopes", { frameId = frame.id }, function(scopes_err, scopes)
      if scopes_err then
        return fail("scopes: " .. vim.inspect(scopes_err))
      end
      local locals_ref = scopes.scopes[1].variablesReference
      session:request("variables", { variablesReference = locals_ref }, function(vars_err, vars)
        if vars_err then
          return fail("variables: " .. vim.inspect(vars_err))
        end
        for _, variable in ipairs(vars.variables) do
          print(string.format("LOCAL %s = %s", variable.name, variable.value))
          if variable.name == "$x" and variable.value == "int(0)" then
            saw_x_zero = true
          end
          if variable.name == "$x" and variable.value == "int(5)" then
            saw_x_five = true
          end
        end
        if stopped == 1 then
          print("STEPPING")
          session:request("next", { threadId = body.threadId }, function(step_err)
            if step_err then
              return fail("next: " .. vim.inspect(step_err))
            end
          end)
        else
          print("CONTINUING")
          session:request("continue", { threadId = body.threadId }, function(continue_err)
            if continue_err then
              return fail("continue: " .. vim.inspect(continue_err))
            end
          end)
        end
      end)
    end)
  end)
end

dap.listeners.after.event_stopped["elephc-smoke"] = function(session, body)
  stopped = stopped + 1
  print(string.format(
    "STOPPED %d reason=%s desc=%s",
    stopped,
    body.reason,
    body.description or ""
  ))
  inspect_then_step(session, body)
end

dap.listeners.after.event_terminated["elephc-smoke"] = function()
  print("TERMINATED")
  done = true
end

dap.listeners.after.event_exited["elephc-smoke"] = function()
  print("EXITED")
  done = true
end

dap.run(dap.configurations.php[1])
vim.wait(20000, function()
  return done
end, 50)

if failed then
  print("FAILED " .. failed)
  vim.cmd("cq")
elseif not done then
  print("FAILED timeout")
  vim.cmd("cq")
elseif stopped ~= 2 or not saw_x_zero or not saw_x_five then
  print(string.format(
    "FAILED stops=%d x0=%s x5=%s",
    stopped,
    tostring(saw_x_zero),
    tostring(saw_x_five)
  ))
  vim.cmd("cq")
else
  print("SMOKE OK")
  vim.cmd("qa!")
end
