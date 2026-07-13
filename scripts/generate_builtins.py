import os
import re
import subprocess

import mistune
from mistune.plugins import plugin_table

BPFTRACE_COMMIT = 'efb8d0d8876295f77170ec9a3c65101d8749f8db'


def _get_bpftrace_docs(doc):
    url = f'https://raw.githubusercontent.com/bpftrace/bpftrace/{BPFTRACE_COMMIT}/docs/{doc}.md'
    r = subprocess.run(['curl', '-sSl', url], capture_output=True, text=True)
    return r.stdout


def _parse_vars_row(row):
    var_name = row['children'][0]
    var_type = row['children'][1]
    description = row['children'][3]

    var_name = var_name['children'][0].get('text', None)
    var_type = var_type['children'][0]['text']
    description = description['children'][0]['text']

    # ignore texts like '$1, $2, ...$n' and 'arg0, arg1, ...argn' for now
    if not var_name or ',' in var_name:
        return None

    return {
        'name': var_name,
        'type': var_type,
        'description': description,
    }


def _parse_vars_table(ast):
    table = next((n for n in ast if n['type'] == 'table'))
    rows = [row for entry in table['children'] for row in entry['children']
                if row['type'] == 'table_row']
    res = []
    for row in rows:
        if var := _parse_vars_row(row):
            res.append(var)
    return res


def _parse_functions_docs(content):
    pattern = re.compile(r'(?s)(?:^|\n)###\s+(.+?)\n(.*?)(?=\n#### |\n### |\n## |\n# |\Z)',
                         re.MULTILINE | re.DOTALL)
    matches = pattern.findall(content)
    result = []
    for heading, content in matches:
        result.append({'name': heading, 'description': content.strip()})
    return result


def export_symbol(var, target):
    # no need to show the details in case of not having any
    if var['description'] == 'n/a':
        var['description'] = ''
    if (not var.get('type')) or var['type'] == 'n/a':
        var['type'] = ''

    print('\t\tBuiltinSymbol {', file=target)
    print('\t\t\tname: "{}",'.format(var['name']), file=target)
    print('\t\t\tdetail: "{}",'.format(var['type']), file=target)
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
    markdown = mistune.create_markdown(renderer=mistune.AstRenderer(),
                                       plugins=[plugin_table])
    ast = markdown(content)
    builtin_vars = _parse_vars_table(ast)
    builtin_funcs = _parse_functions_docs(content)

    lang_content = _get_bpftrace_docs('language')
    config_vars = _parse_config_vars(lang_content)

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
