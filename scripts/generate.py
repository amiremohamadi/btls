import os
import re
import subprocess

import mistune
from mistune.plugins import plugin_table


def _get_bpftrace_stdlib_docs():
    url = 'https://raw.githubusercontent.com/bpftrace/bpftrace/efb8d0d8876295f77170ec9a3c65101d8749f8db/docs/stdlib.md'
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


def generate_builtins():
    with open('./stdlib.md', 'r') as f:
        content = f.read()
        markdown = mistune.create_markdown(renderer=mistune.AstRenderer(),
                                           plugins=[plugin_table])
        ast = markdown(content)
        builtin_vars = _parse_vars_table(ast)
        builtin_funcs = _parse_functions_docs(content)

    with open('./target/builtins.gen.rs', 'w') as target:
        print('// DO NOT EDIT -- this file is auto generated\n',
              file=target)
        print('BuiltinSymbols {', file=target)
        export_symbols('keywords', builtin_vars, target)
        export_symbols('functions', builtin_funcs, target)
        print('}', file=target)


def main():
    # os.chdir(os.path.dirname(os.path.dirname(__file__)))
    # print(_get_bpftrace_stdlib_docs())
    generate_builtins()


if __name__ == '__main__':
    main()
