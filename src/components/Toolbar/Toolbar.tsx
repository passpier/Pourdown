import { Editor } from '@tiptap/react';

type HeadingLevel = 1 | 2 | 3 | 4 | 5 | 6;
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { Tooltip } from '@/components/ui/tooltip';
import { DropdownMenu, DropdownMenuItem } from '@/components/ui/dropdown-menu';
import {
  Bold,
  Italic,
  Strikethrough,
  Code,
  List,
  ListOrdered,
  Quote,
  Minus,
  ChevronDown,
  Table,
  Plus,
  Trash2,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { insertTable, insertRowBelow, deleteRow, insertColumnRight, deleteColumn } from '@/lib/tableCommands';

interface ToolbarProps {
  editor: Editor | null;
}

export function Toolbar({ editor }: ToolbarProps) {
  const { t } = useTranslation();
  if (!editor) {
    return null;
  }

  return (
    <div className="border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex items-center gap-1 p-2">
        {/* Heading Dropdown */}
        <DropdownMenu
          trigger={
            <Button variant="ghost" size="sm" className="h-9">
              <span className="text-sm">
                {editor.isActive('heading', { level: 1 }) ? 'H1' :
                 editor.isActive('heading', { level: 2 }) ? 'H2' :
                 editor.isActive('heading', { level: 3 }) ? 'H3' :
                 editor.isActive('heading', { level: 4 }) ? 'H4' :
                 editor.isActive('heading', { level: 5 }) ? 'H5' :
                 editor.isActive('heading', { level: 6 }) ? 'H6' :
                 t('toolbar.normal')}
              </span>
              <ChevronDown className="w-4 h-4 ml-1" />
            </Button>
          }
        >
          <DropdownMenuItem
            onClick={() => editor.chain().focus().setParagraph().run()}
          >
            {t('toolbar.normal')}
          </DropdownMenuItem>
          {[1, 2, 3, 4, 5, 6].map((level) => (
            <DropdownMenuItem
              key={level}
              onClick={() => editor.chain().focus().toggleHeading({ level: level as HeadingLevel }).run()}
            >
              {t('toolbar.heading', { level })}
            </DropdownMenuItem>
          ))}
        </DropdownMenu>

        <Separator orientation="vertical" className="h-6 mx-1" />

        {/* Text Formatting */}
        <Tooltip content={t('toolbar.bold')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleBold().run()}
            className={cn(editor.isActive('bold') && 'bg-accent')}
          >
            <Bold className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Tooltip content={t('toolbar.italic')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleItalic().run()}
            className={cn(editor.isActive('italic') && 'bg-accent')}
          >
            <Italic className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Tooltip content={t('toolbar.strikethrough')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleStrike().run()}
            className={cn(editor.isActive('strike') && 'bg-accent')}
          >
            <Strikethrough className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Tooltip content={t('toolbar.inline_code')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleCode().run()}
            className={cn(editor.isActive('code') && 'bg-accent')}
          >
            <Code className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Separator orientation="vertical" className="h-6 mx-1" />

        {/* Lists */}
        <Tooltip content={t('toolbar.bullet_list')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleBulletList().run()}
            className={cn(editor.isActive('bulletList') && 'bg-accent')}
          >
            <List className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Tooltip content={t('toolbar.ordered_list')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleOrderedList().run()}
            className={cn(editor.isActive('orderedList') && 'bg-accent')}
          >
            <ListOrdered className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Separator orientation="vertical" className="h-6 mx-1" />

        {/* Block Elements */}
        <Tooltip content={t('toolbar.blockquote')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleBlockquote().run()}
            className={cn(editor.isActive('blockquote') && 'bg-accent')}
          >
            <Quote className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Tooltip content={t('toolbar.code_block')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().toggleCodeBlock().run()}
            className={cn(editor.isActive('codeBlock') && 'bg-accent')}
          >
            <Code className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Tooltip content={t('toolbar.horizontal_rule')}>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => editor.chain().focus().setHorizontalRule().run()}
          >
            <Minus className="w-4 h-4" />
          </Button>
        </Tooltip>

        <Separator orientation="vertical" className="h-6 mx-1" />

        {/* Table */}
        <DropdownMenu
          trigger={
            <Button 
              variant="ghost" 
              size="icon"
              className={cn(editor.isActive('table') && 'bg-accent')}
              title={t('toolbar.insert_table')}
            >
              <Table className="w-4 h-4" />
              <ChevronDown className="w-3 h-3 ml-0.5" />
            </Button>
          }
        >
          <DropdownMenuItem
            onClick={() => insertTable(editor, { rows: 3, cols: 3 })}
          >
            <span>{t('toolbar.insert_table')}</span>
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => insertTable(editor, { rows: 2, cols: 2 })}
          >
            <span>{t('toolbar.table_2x2')}</span>
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => insertTable(editor, { rows: 3, cols: 3 })}
          >
            <span>{t('toolbar.table_3x3')}</span>
          </DropdownMenuItem>
          <DropdownMenuItem
            onClick={() => insertTable(editor, { rows: 4, cols: 4 })}
          >
            <span>{t('toolbar.table_4x4')}</span>
          </DropdownMenuItem>
          {editor.isActive('table') && (
            <>
              <Separator className="my-1" />
              <DropdownMenuItem onClick={() => insertRowBelow(editor)}>
                <Plus className="w-4 h-4 mr-2" />
                <span>{t('toolbar.add_row')}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => deleteRow(editor)}>
                <Trash2 className="w-4 h-4 mr-2" />
                <span>{t('toolbar.delete_row')}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => insertColumnRight(editor)}>
                <Plus className="w-4 h-4 mr-2" />
                <span>{t('toolbar.add_column')}</span>
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => deleteColumn(editor)}>
                <Trash2 className="w-4 h-4 mr-2" />
                <span>{t('toolbar.delete_column')}</span>
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenu>
      </div>
    </div>
  );
}
