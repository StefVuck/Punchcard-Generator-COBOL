use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead};
use clap::Parser;

// IBM punch card dimensions in mm
const CARD_WIDTH_MM: f32 = 187.325;
const CARD_HEIGHT_MM: f32 = 82.55;

// A4 dimensions in mm
const A4_WIDTH_MM: f32 = 210.0;
const A4_HEIGHT_MM: f32 = 297.0;

// Punch card has 80 columns and 12 rows
const COLUMNS: usize = 80;
const ROWS: usize = 12;

// Cards per page
const CARDS_PER_PAGE: usize = 3;

// Points per mm
const PT_PER_MM: f32 = 2.834645;

// Template punch hole positions (in pixels from template image)
const FIRST_PUNCH_X: f32 = 30.0;  // X position of first column
const FIRST_PUNCH_Y: f32 = 25.0;  // Y position of first row (12-punch)
const COLUMN_SPACING: f32 = 9.0; // Pixels between columns
const ROW_SPACING: f32 = 27.0;    // Pixels between rows
const PUNCH_WIDTH_PX: f32 = 7.0;  // Punch width in pixels
const PUNCH_HEIGHT_PX: f32 = 15.0; // Punch height in pixels

fn get_hollerith_encoding() -> HashMap<char, Vec<usize>> {
    let mut map = HashMap::new();
    
    // Letters A-I: 12-punch + 1-9
    for (i, c) in ('A'..='I').enumerate() {
        map.insert(c, vec![0, i + 3]);
    }
    
    // Letters J-R: 11-punch + 1-9
    for (i, c) in ('J'..='R').enumerate() {
        map.insert(c, vec![1, i + 3]);
    }
    
    // Letters S-Z: 0-punch + 2-9
    for (i, c) in ('S'..='Z').enumerate() {
        map.insert(c, vec![2, i + 4]);
    }
    
    // Digits 0-9
    for i in 0..10 {
        let digit = char::from_digit(i, 10).unwrap();
        map.insert(digit, vec![i as usize + 2]);
    }
    
    // Special characters (simplified set)
    map.insert(' ', vec![]);  // No punches for space
    map.insert('.', vec![0, 1, 10]);  // 12-11-8
    map.insert(',', vec![0, 5]);      // 12-3
    map.insert('(', vec![0, 7]);      // 12-5
    map.insert(')', vec![1, 7]);      // 11-5
    map.insert('+', vec![0, 8]);      // 12-6
    map.insert('-', vec![1]);         // 11
    map.insert('*', vec![1, 6]);      // 11-4
    map.insert('/', vec![2, 3]);      // 0-1
    map.insert('=', vec![0, 8]);      // 12-6 (same as +)
    map.insert('$', vec![1, 5]);      // 11-3
    map.insert('\'', vec![0, 10]);    // 12-8
    map.insert(':', vec![4, 10]);     // 2-8
    map.insert(';', vec![0, 1, 8]);   // 12-11-6
    map.insert('"', vec![0, 10]);     // 12-8
    
    map
}

struct PunchCard {
    columns: Vec<Vec<usize>>,  // For each column, which rows to punch
}

impl PunchCard {
    fn new() -> Self {
        PunchCard {
            columns: vec![vec![]; COLUMNS],
        }
    }
    
    fn from_cobol_line(line: &str, sequence_num: usize, encoding_map: &HashMap<char, Vec<usize>>) -> Self {
        let mut card = PunchCard::new();
        
        // Format the line with proper COBOL columns:
        // Columns 1-6: Sequence number (right-aligned, zero-padded)
        // Column 7: Indicator area (preserved from input or space)
        // Columns 8-72: COBOL code
        // Columns 73-80: Sequence/identification (card sequence number)
        
        let sequence_str = format!("{:06}", sequence_num % 1000000);
        let card_seq_str = format!("{:08}", sequence_num);
        
        // Check if line starts with spaces (typical COBOL indentation)
        // If it starts with 7+ spaces, it's likely already formatted
        let starts_with_spaces = line.starts_with("       "); // 7 spaces
        
        let (indicator, code_part) = if starts_with_spaces && line.len() > 7 {
            // Line has leading spaces - treat as formatted
            // Extract indicator from position 6 and code from position 7
            let indicator = line.chars().nth(6).unwrap_or(' ');
            let code = line[7..].trim_end();
            (indicator, code.to_string())
        } else {
            // No leading spaces or too short - treat entire line as code
            (' ', line.trim().to_string())
        };
        
        // Build the full 80-column line
        let mut formatted = String::with_capacity(80);
        formatted.push_str(&sequence_str);           // Columns 1-6
        formatted.push(indicator);                    // Column 7
        formatted.push_str(&format!("{:<65}", code_part)); // Columns 8-72 (65 chars)
        formatted.push_str(&card_seq_str);           // Columns 73-80
        
        // Ensure exactly 80 characters
        let final_line = format!("{:<80}", formatted.chars().take(80).collect::<String>());
        
        // Encode each column
        for (col_idx, ch) in final_line.chars().enumerate() {
            let uppercase_ch = ch.to_uppercase().next().unwrap();
            
            if let Some(punches) = encoding_map.get(&uppercase_ch) {
                card.columns[col_idx] = punches.clone();
            } else {
                // Unknown character - leave blank
                card.columns[col_idx] = vec![];
            }
        }
        
        card
    }
}

fn validate_and_format_cobol(lines: Vec<String>) -> Result<Vec<String>, String> {
    let mut formatted_lines = Vec::new();
    
    for (line_num, line) in lines.iter().enumerate() {
        // Remove any trailing whitespace but preserve leading structure
        let trimmed = line.trim_end().to_string();
        
        // Check if line is too long (COBOL lines shouldn't exceed 80 columns)
        if trimmed.len() > 80 {
            return Err(format!(
                "Line {} exceeds 80 columns ({} chars): {}",
                line_num + 1,
                trimmed.len(),
                &trimmed[..std::cmp::min(40, trimmed.len())]
            ));
        }
        
        // Handle blank lines
        if trimmed.is_empty() {
            formatted_lines.push(String::new());
            continue;
        }
        
        // Check for comment lines (asterisk in column 7)
        if trimmed.len() >= 7 && trimmed.chars().nth(6) == Some('*') {
            formatted_lines.push(trimmed);
            continue;
        }
        
        // If the line starts with 6 digits, assume it's already formatted
        let already_formatted = trimmed.len() >= 6 
            && trimmed[..6].chars().all(|c| c.is_numeric() || c == ' ');
        
        if already_formatted {
            formatted_lines.push(trimmed);
        } else {
            // Raw COBOL code - will be formatted when creating punch card
            formatted_lines.push(trimmed);
        }
    }
    
    println!("Validated {} lines of COBOL code", formatted_lines.len());
    Ok(formatted_lines)
}

/// Generate JCL for compiling and running the COBOL program
fn generate_jcl(program_name: &str, cobol_line_count: usize) -> Vec<String> {
    let mut jcl = Vec::new();
    
    // Job card
    jcl.push(format!("//{}    JOB (ACCT),'COBOL COMPILE',CLASS=A,MSGCLASS=A", 
        program_name.to_uppercase()));
    jcl.push("//             MSGLEVEL=(1,1),NOTIFY=&SYSUID".to_string());
    
    // Step 1: Compile the COBOL program
    jcl.push("//*".to_string());
    jcl.push("//COMPILE  EXEC PGM=IGYCRCTL,REGION=0M".to_string());
    jcl.push("//STEPLIB  DD DSNAME=IGY.V6R3M0.SIGYCOMP,DISP=SHR".to_string());
    jcl.push("//SYSPRINT DD SYSOUT=*".to_string());
    jcl.push("//SYSLIN   DD DSNAME=&&LOADSET,DISP=(MOD,PASS),".to_string());
    jcl.push("//            UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT1   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT2   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT3   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT4   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT5   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT6   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSUT7   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSIN    DD *".to_string());
    
    jcl.push(format!("//* {} COBOL SOURCE CARDS FOLLOW", cobol_line_count));
    jcl.push("/*".to_string());
    
    // Step 2: Link-edit the compiled program
    jcl.push("//*".to_string());
    jcl.push("//LKED     EXEC PGM=IEWL,PARM='LIST,XREF,LET',".to_string());
    jcl.push("//             REGION=1024K".to_string());
    jcl.push("//SYSLIB   DD DSNAME=CEE.SCEELKED,DISP=SHR".to_string());
    jcl.push("//SYSLIN   DD DSNAME=&&LOADSET,DISP=(OLD,DELETE)".to_string());
    jcl.push("//SYSLMOD  DD DSNAME=&&GOSET(GO),DISP=(NEW,PASS),".to_string());
    jcl.push("//            UNIT=SYSDA,SPACE=(CYL,(1,1,1))".to_string());
    jcl.push("//SYSUT1   DD UNIT=SYSDA,SPACE=(CYL,(1,1))".to_string());
    jcl.push("//SYSPRINT DD SYSOUT=*".to_string());
    
    // Step 3: Execute the program
    jcl.push("//*".to_string());
    jcl.push("//GO       EXEC PGM=*.LKED.SYSLMOD".to_string());
    jcl.push("//STEPLIB  DD DSNAME=CEE.SCEERUN,DISP=SHR".to_string());
    jcl.push("//SYSOUT   DD SYSOUT=*".to_string());
    jcl.push("//SYSPRINT DD SYSOUT=*".to_string());
    jcl.push("//SYSUDUMP DD SYSOUT=*".to_string());
    jcl.push("//SYSIN    DD *".to_string());
    jcl.push("//* INPUT DATA CARDS (IF ANY)".to_string());
    jcl.push("/*".to_string());
    jcl.push("//".to_string());
    
    jcl
}

/// Extract program name from COBOL source
fn extract_program_name(cobol_lines: &[String]) -> String {
    for line in cobol_lines {
        let upper = line.to_uppercase();
        if upper.contains("PROGRAM-ID") {
            // Try to extract the program name after PROGRAM-ID.
            if let Some(pos) = upper.find("PROGRAM-ID") {
                let after = &line[pos + 10..];
                // Skip the period and whitespace
                let name = after.trim_start_matches('.').trim();
                // Take first word
                let program_name = name.split_whitespace().next().unwrap_or("COBPROG");
                return program_name.replace(".", "").to_uppercase();
            }
        }
    }
    "COBPROG".to_string() // Default name
}

/// Generate a text representation like a coding sheet
fn generate_coding_sheet(cobol_lines: &[String]) -> String {
    let mut output = String::new();
    
    // Header
    output.push_str("================================================================================\n");
    output.push_str("                            COBOL CODING SHEET                                  \n");
    output.push_str("================================================================================\n");
    output.push_str("SEQ   IND         COBOL CODE (Columns 8-72)                             CARD    \n");
    output.push_str("1-6   78       16      24      32      40      48      56      64       73-80   \n");
    output.push_str("--------------------------------------------------------------------------------\n");
    
    for (idx, line) in cobol_lines.iter().enumerate() {
        let sequence_num = idx + 1;
        
        // Use the same logic as PunchCard::from_cobol_line
        let starts_with_spaces = line.starts_with("       "); // 7 spaces
        
        let (indicator, code_part) = if starts_with_spaces && line.len() > 7 {
            let ind = line.chars().nth(6).unwrap_or(' ');
            let code = line[7..].trim_end();
            (ind, code.to_string())
        } else {
            (' ', line.trim().to_string())
        };
        
        let sequence_str = format!("{:06}", sequence_num % 1000000);
        let card_seq_str = format!("{:08}", sequence_num);
        
        // Format the line with column markers
        output.push_str(&format!("{}  {}  {:<65}  {}\n", 
            sequence_str, 
            indicator, 
            code_part,
            card_seq_str
        ));
    }
    
    output.push_str("================================================================================\n");
    output.push_str(&format!("Total Cards: {}\n", cobol_lines.len()));
    output.push_str("================================================================================\n");
    
    output
}

fn generate_punch_card_pdf(
    cobol_lines: Vec<String>,
    template_path: &str,
    output_path: &str,
    coding_sheet_path: &str,
    include_jcl: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    
    let encoding_map = get_hollerith_encoding();
    
    // Extract program name and generate JCL if requested
    let mut all_lines = Vec::new();
    
    if include_jcl {
        let program_name = extract_program_name(&cobol_lines);
        println!("Program name detected: {}", program_name);
        
        let jcl_lines = generate_jcl(&program_name, cobol_lines.len());
        
        // Add JCL header cards
        for line in &jcl_lines {
            // Stop before the marker for COBOL source
            if line.contains("COBOL SOURCE CARDS FOLLOW") {
                break;
            }
            all_lines.push(line.clone());
        }
        
        // Add COBOL source cards
        all_lines.extend(cobol_lines.clone());
        
        // Add remaining JCL cards (after the COBOL source marker)
        let mut after_marker = false;
        for line in &jcl_lines {
            if after_marker {
                all_lines.push(line.clone());
            }
            if line.contains("COBOL SOURCE CARDS FOLLOW") {
                after_marker = true;
            }
        }
        
        println!("Total cards (with JCL): {}", all_lines.len());
    } else {
        all_lines = cobol_lines.clone();
        println!("Total cards (COBOL only): {}", all_lines.len());
    }
    
    // Generate coding sheet text file
    let coding_sheet_text = generate_coding_sheet(&all_lines);
    fs::write(coding_sheet_path, coding_sheet_text)?;
    println!("✓ Coding sheet generated: {}", coding_sheet_path);
    
    // Load template image
    let img = image::open(template_path)?;
    let img_rgb = img.to_rgb8();
    let (img_width, img_height) = img_rgb.dimensions();
    
    // Convert cards with sequence numbers
    let cards: Vec<PunchCard> = all_lines
        .iter()
        .enumerate()
        .map(|(idx, line)| PunchCard::from_cobol_line(line, idx + 1, &encoding_map))
        .collect();
    
    // Use lopdf for manual PDF construction
    use lopdf::{Document, Object, Stream, Dictionary};
    
    let mut doc = Document::with_version("1.5");
    
    // Calculate dimensions in points
    let page_width = A4_WIDTH_MM * PT_PER_MM;
    let page_height = A4_HEIGHT_MM * PT_PER_MM;
    let card_width_pt = CARD_WIDTH_MM * PT_PER_MM;
    let card_height_pt = CARD_HEIGHT_MM * PT_PER_MM;
    let margin_left = ((A4_WIDTH_MM - CARD_WIDTH_MM) / 2.0) * PT_PER_MM;
    let spacing = ((A4_HEIGHT_MM - (CARD_HEIGHT_MM * CARDS_PER_PAGE as f32)) / (CARDS_PER_PAGE as f32 + 1.0)) * PT_PER_MM;
    
    // Add template image as XObject
    let image_data = img_rgb.as_raw().clone();
    let mut image_dict = Dictionary::new();
    image_dict.set("Type", Object::Name(b"XObject".to_vec()));
    image_dict.set("Subtype", Object::Name(b"Image".to_vec()));
    image_dict.set("Width", Object::Integer(img_width as i64));
    image_dict.set("Height", Object::Integer(img_height as i64));
    image_dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
    image_dict.set("BitsPerComponent", Object::Integer(8));
    
    let image_stream = Stream::new(image_dict, image_data);
    let image_id = doc.add_object(image_stream);
    
    // Process cards in pages
    for page_cards in cards.chunks(CARDS_PER_PAGE) {
        let mut operations = Vec::new();
        
        // Draw each card on this page
        for (card_position, card) in page_cards.iter().enumerate() {
            // Calculate Y position (from bottom in PDF coordinates)
            let y_pos = page_height - spacing - ((card_position as f32 + 1.0) * card_height_pt) - (card_position as f32 * spacing);
            
            // Draw template image
            operations.push(("q".to_string(), vec![])); // Save graphics state
            operations.push((
                "cm".to_string(),
                vec![
                    card_width_pt.into(),
                    0.0.into(),
                    0.0.into(),
                    card_height_pt.into(),
                    margin_left.into(),
                    y_pos.into(),
                ],
            )); // Transform matrix
            operations.push(("Do".to_string(), vec![Object::Name(format!("Im{}", image_id.0).into_bytes())]));
            operations.push(("Q".to_string(), vec![])); // Restore graphics state
            
            // Calculate punch hole dimensions using template coordinates
            // The template image coordinates need to be transformed to match how the image is placed in the PDF
            let scale_x = card_width_pt / img_width as f32;
            let scale_y = card_height_pt / img_height as f32;
            
            let punch_width_pt = PUNCH_WIDTH_PX * scale_x;
            let punch_height_pt = PUNCH_HEIGHT_PX * scale_y;
            
            // Set black fill color
            operations.push(("rg".to_string(), vec![0.0.into(), 0.0.into(), 0.0.into()]));
            
            // Draw punches as black rectangles using template coordinates
            for (col_idx, punches) in card.columns.iter().enumerate() {
                for &row_idx in punches {
                    // Calculate position based on template pixel coordinates
                    let punch_x_px = FIRST_PUNCH_X + (col_idx as f32 * COLUMN_SPACING);
                    let punch_y_px = FIRST_PUNCH_Y + (row_idx as f32 * ROW_SPACING);
                    
                    // Convert template image coordinates to PDF coordinates
                    // X: straightforward - template X position scaled and offset by card position
                    let x = margin_left + (punch_x_px * scale_x);
                    
                    // Y: The template image is drawn with its bottom-left at y_pos
                    // In the image, Y increases downward from top
                    // In PDF, Y increases upward from bottom
                    // So: PDF_Y = y_pos + (img_height - template_Y - punch_height) * scale
                    let punch_y = y_pos + ((img_height as f32 - punch_y_px) * scale_y) - punch_height_pt;
                    
                    // Rectangle: x y width height re
                    operations.push((
                        "re".to_string(),
                        vec![
                            x.into(),
                            punch_y.into(),
                            punch_width_pt.into(),
                            punch_height_pt.into(),
                        ],
                    ));
                    operations.push(("f".to_string(), vec![])); // Fill
                }
            }
        }
        
        // Encode operations into content stream
        let mut content_data = Vec::new();
        for (operator, operands) in operations {
            for operand in operands {
                // Manually serialize Object to bytes
                match operand {
                    Object::Integer(i) => content_data.extend_from_slice(i.to_string().as_bytes()),
                    Object::Real(f) => content_data.extend_from_slice(f.to_string().as_bytes()),
                    Object::Name(ref n) => {
                        content_data.push(b'/');
                        content_data.extend_from_slice(n);
                    },
                    Object::String(ref s, _) => {
                        content_data.push(b'(');
                        content_data.extend_from_slice(s);
                        content_data.push(b')');
                    },
                    Object::Reference(r) => {
                        content_data.extend_from_slice(r.0.to_string().as_bytes());
                        content_data.push(b' ');
                        content_data.extend_from_slice(r.1.to_string().as_bytes());
                        content_data.push(b' ');
                        content_data.push(b'R');
                    },
                    _ => {},
                }
                content_data.push(b' ');
            }
            content_data.extend_from_slice(operator.as_bytes());
            content_data.push(b'\n');
        }
        
        // Create page
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content_data));
        
        let mut resources = Dictionary::new();
        let mut xobjects = Dictionary::new();
        xobjects.set(format!("Im{}", image_id.0), Object::Reference(image_id));
        resources.set("XObject", Object::Dictionary(xobjects));
        
        let mut page_dict = Dictionary::new();
        page_dict.set("Type", Object::Name(b"Page".to_vec()));
        page_dict.set("MediaBox", vec![0.into(), 0.into(), page_width.into(), page_height.into()]);
        page_dict.set("Contents", Object::Reference(content_id));
        page_dict.set("Resources", Object::Dictionary(resources));
        
        let _page_id = doc.add_object(page_dict);
    }
    
    // After all pages are added, build the page tree manually
    let page_ids: Vec<_> = doc.objects.iter()
        .filter(|(_, obj)| {
            if let Object::Dictionary(dict) = obj {
                if let Ok(obj_type) = dict.get(b"Type") {
                    matches!(obj_type, Object::Name(name) if name == b"Page")
                } else {
                    false
                }
            } else {
                false
            }
        })
        .map(|(id, _)| *id)
        .collect();
    
    // Create Pages object with all page references
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", Object::Array(
        page_ids.iter().map(|id| Object::Reference(*id)).collect()
    ));
    pages_dict.set("Count", Object::Integer(page_ids.len() as i64));
    let pages_id = doc.add_object(pages_dict);
    
    // Update all pages to reference the Pages object as parent
    for page_id in page_ids {
        if let Ok(page_obj) = doc.get_object_mut(page_id) {
            if let Object::Dictionary(page_dict) = page_obj {
                page_dict.set("Parent", Object::Reference(pages_id));
            }
        }
    }
    
    // Find or create the catalog object ID
    let catalog_id = doc.objects.iter()
        .find(|(_, obj)| {
            if let Object::Dictionary(dict) = obj {
                if let Ok(obj_type) = dict.get(b"Type") {
                    matches!(obj_type, Object::Name(name) if name == b"Catalog")
                } else {
                    false
                }
            } else {
                false
            }
        })
        .map(|(id, _)| *id)
        .unwrap_or_else(|| {
            // No catalog exists, create one
            let mut catalog = Dictionary::new();
            catalog.set("Type", Object::Name(b"Catalog".to_vec()));
            let catalog_id = doc.add_object(catalog);
            
            // Set it as the root in trailer
            doc.trailer.set("Root", Object::Reference(catalog_id));
            catalog_id
        });
    
    // Now update the catalog with Pages reference
    if let Ok(catalog_obj) = doc.get_object_mut(catalog_id) {
        if let Object::Dictionary(catalog_dict) = catalog_obj {
            catalog_dict.set("Pages", Object::Reference(pages_id));
        }
    }
    
    // Save PDF
    doc.save(output_path)?;
    
    Ok(())
}

/// COBOL to Punch Card PDF Generator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// COBOL source file to process
    #[arg(short, long)]
    input: String,
    
    /// Output PDF file path
    #[arg(short, long, default_value = "output.pdf")]
    output: String,
    
    /// Punch card template image file
    #[arg(short, long, default_value = "punchcard_template.png")]
    template: String,
    
    /// Coding sheet text output file
    #[arg(short, long, default_value = "coding_sheet.txt")]
    coding_sheet: String,
    
    /// Include JCL (Job Control Language) wrapper
    #[arg(short, long, default_value_t = false)]
    jcl: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    println!("COBOL to Punch Card PDF Generator");
    println!("==================================");
    println!("Input file:      {}", args.input);
    println!("Output PDF:      {}", args.output);
    println!("Coding sheet:    {}", args.coding_sheet);
    println!("Include JCL:     {}", if args.jcl { "Yes" } else { "No" });
    println!();
    
    println!("Reading COBOL file: {}", args.input);
    let file = fs::File::open(&args.input)?;
    let reader = io::BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    
    println!("Validating and formatting COBOL...");
    let formatted_lines = validate_and_format_cobol(lines)?;
    
    println!("Processing {} lines of COBOL...", formatted_lines.len());
    
    if args.jcl {
        println!("Generating JCL wrapper...");
    }
    
    generate_punch_card_pdf(
        formatted_lines, 
        &args.template, 
        &args.output, 
        &args.coding_sheet,
        args.jcl
    )?;
    
    println!();
    println!("✓ Punch cards generated successfully!");
    println!("  PDF:           {}", args.output);
    println!("  Coding sheet:  {}", args.coding_sheet);
    
    Ok(())
}

// Cargo.toml dependencies needed:
// [dependencies]
// lopdf = "0.32"
// image = "0.24"
// clap = { version = "4.5", features = ["derive"] }
