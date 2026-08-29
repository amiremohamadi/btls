import os
import re
import subprocess

BPFTRACE_COMMIT = 'd052fff44649305ddcb9c5e6d19f74c2e41e542f'

FUNC_SECTION = re.compile(r'^### (\w+)\n(.*?)(?=^### \w|^## |\Z)', re.M | re.S)
SIG_LINE = re.compile(r'^-\s+`([^`]+)`\s*$')


def _get_bpftrace_docs(doc):
    url = f'https://raw.githubusercontent.com/bpftrace/bpftrace/{BPFTRACE_COMMIT}/docs/{doc}.md'
    r = subprocess.run(['curl', '-sSl', url], capture_output=True, text=True)
    return r.stdout


def _parse_functions_docs(content):
    funcs = []
    vars = []
    for heading, body in FUNC_SECTION.findall(content):
        body = body.strip()
        funcs.append({'name': heading, 'description': body})
        for line in body.splitlines():
            m = SIG_LINE.match(line.strip())
            if not m or '(' in m.group(1):
                continue
            vars.append({
                'name': heading,
                'type': ' '.join(m.group(1).rstrip(';').split()[:-1]),
                'description': body,
            })
            break
    return funcs, vars


def export_symbol(var, target):
    print('\t\tBuiltinSymbol {', file=target)
    print('\t\t\tname: "{}",'.format(var['name']), file=target)
    print('\t\t\tdetail: "{}",'.format(var.get('type', '')), file=target)
    print('\t\t\tdocumentation: r#"{}"#,'.format(var['description']), file=target)
    print('\t\t},', file=target)


def export_symbols(field, symbols, target):
    print('\t{}: &['.format(field), file=target)
    for sym in symbols:
        export_symbol(sym, target)
    print('\t],', file=target)


def _parse_config_vars(content):
    section = re.search(
        r'^## Config Variables\n(.*?)(?=\n## |\Z)',
        content,
        re.MULTILINE | re.DOTALL,
    )
    if not section:
        return []
    section = section.group(1)

    vars = []
    for m in re.finditer(
        r'^### (\w+)\n(.*?)(?=\n### |\Z)',
        section,
        re.MULTILINE | re.DOTALL,
    ):
        name = m.group(1)
        body = m.group(2).strip()

        default_m = re.search(r'^Default:\s*(.+?)$', body, re.MULTILINE)
        default = default_m.group(1).strip().strip('`').strip('"') if default_m else ''

        values = []
        for vm in re.finditer(
            r'^[-*]\s+`?(\S+?)`?(?:\s*[:-]\s.*)?$',
            body,
            re.MULTILINE,
        ):
            val = vm.group(1)
            if val not in values:
                values.append(val)

        # build detail string
        if values:
            detail = ' | '.join(values)
            if default and default in values:
                detail += f' (default: {default})'
        elif default in ('true', 'false'):
            detail = f'bool (default: {default})'
            values = ['true', 'false']
        elif default.isdigit():
            detail = f'number (default: {default})'
        elif default:
            detail = f'string (default: "{default}")'
        else:
            inline = re.search(r"defaults to `(\w+)`", body)
            if inline:
                val = inline.group(1)
                detail = f'bool (default: {val})'
                values = ['true', 'false']
            else:
                detail = ''

        vars.append({'name': name, 'detail': detail, 'documentation': body, 'values': values})

    return vars


def export_config_var(var, target):
    print('\t\tConfigVar {', file=target)
    print('\t\t\tname: "{}",'.format(var['name']), file=target)
    print('\t\t\tdetail: r#"{}"#,'.format(var['detail']), file=target)
    print('\t\t\tdocumentation: r#"{}"#,'.format(var['documentation']), file=target)
    print('\t\t\tvalues: &[', file=target)
    for val in var['values']:
        print('\t\t\t\t"{}",'.format(val), file=target)
    print('\t\t\t],', file=target)
    print('\t\t},', file=target)


def export_config_vars(field, vars, target):
    print('\t{}: &['.format(field), file=target)
    for var in vars:
        export_config_var(var, target)
    print('\t],', file=target)


def generate_builtins():
    content = _get_bpftrace_docs('stdlib')
    builtin_funcs, builtin_vars = _parse_functions_docs(content)
    config_vars = _parse_config_vars(_get_bpftrace_docs('language'))

    with open('./target/builtins.gen.rs', 'w') as target:
        print('// DO NOT EDIT -- this file is auto generated\n',
              file=target)
        print('BuiltinSymbols {', file=target)
        export_symbols('keywords', builtin_vars, target)
        export_symbols('functions', builtin_funcs, target)
        export_config_vars('config_vars', config_vars, target)
        print('}', file=target)


def main():
    root = os.path.dirname(os.path.dirname(__file__))
    target_path = f'{root}/target/builtins.gen.rs'
    if not os.path.exists(target_path):
        print('generating...')
        generate_builtins()
        print(f'generated "{target_path}"')
    else:
        print('builtins already exists!')
        print(f'remove "{target_path}" if you want to re-generate.')


if __name__ == '__main__':
    main()
