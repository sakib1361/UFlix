using System.Windows.Input;

namespace UFlix.Engine.Models
{
    public class RelayCommand : ICommand
    {
        private Action<object> execute;

        private Predicate<object> canExecute;

        public event EventHandler CanExecuteChanged;

        public RelayCommand(Action execute) : this(new Action<object>(x => execute?.Invoke()), DefaultCanExecute) { }
        public RelayCommand(Action<object> execute) : this(execute, DefaultCanExecute) { }

        public RelayCommand(Action<object> execute, Predicate<object> canExecute)
        {
            this.execute    = execute ?? throw new ArgumentNullException(nameof(execute));
            this.canExecute = canExecute ?? throw new ArgumentNullException(nameof(canExecute));
        }

        private static bool DefaultCanExecute(object parameter) => true;
        public bool CanExecute(object parameter) => canExecute != null && canExecute(parameter);

        public void Execute(object parameter) => execute(parameter);

        public void OnCanExecuteChanged()
        {
            var handler = CanExecuteChanged;
            handler?.Invoke(this, EventArgs.Empty);
            //CommandManager.InvalidateRequerySuggested();
        }

        public void Destroy()
        {
            canExecute = _ => false;
            execute = _ => { return; };
        }
    }
}
